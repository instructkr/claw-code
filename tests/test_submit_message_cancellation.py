"""Tests for cooperative cancellation in submit_message (ROADMAP #164 Stage A).

Verifies that cancel_event enables safe early termination:
- Event set before call => immediate return with stop_reason='cancelled'
- Event set between budget check and commit => still 'cancelled', no mutation
- Event set after commit => not observable (honest cooperative limit)
- Legacy callers (cancel_event=None) see zero behaviour change
- State is untouched on cancellation: mutable_messages, transcript_store,
  permission_denials, total_usage all preserved

This closes the #161 follow-up gap filed as #164: wedged provider threads
can no longer silently commit ghost turns after the caller observed a
timeout.
"""

from __future__ import annotations

import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.models import PermissionDenial  # noqa: E402
from src.port_manifest import build_port_manifest  # noqa: E402
from src.query_engine import QueryEngineConfig, QueryEnginePort, TurnResult  # noqa: E402


def _fresh_engine(**config_overrides) -> QueryEnginePort:
    config = QueryEngineConfig(**config_overrides) if config_overrides else QueryEngineConfig()
    return QueryEnginePort(manifest=build_port_manifest(), config=config)


class TestCancellationBeforeCall:
    """Event set before submit_message is invoked => immediate 'cancelled'."""

    def test_pre_set_event_returns_cancelled_immediately(self) -> None:
        engine = _fresh_engine()
        event = threading.Event()
        event.set()

        result = engine.submit_message('hello', cancel_event=event)

        assert result.stop_reason == 'cancelled'
        assert result.prompt == 'hello'
        # Output is empty on pre-budget cancel (no synthesis)
        assert result.output == ''

    def test_pre_set_event_preserves_mutable_messages(self) -> None:
        engine = _fresh_engine()
        event = threading.Event()
        event.set()

        engine.submit_message('ghost turn', cancel_event=event)

        assert engine.mutable_messages == [], (
            'cancelled turn must not appear in mutable_messages'
        )

    def test_pre_set_event_preserves_transcript_store(self) -> None:
        engine = _fresh_engine()
        event = threading.Event()
        event.set()

        engine.submit_message('ghost turn', cancel_event=event)

        assert engine.transcript_store.entries == [], (
            'cancelled turn must not appear in transcript_store'
        )

    def test_pre_set_event_preserves_usage_counters(self) -> None:
        engine = _fresh_engine()
        initial_usage = engine.total_usage
        event = threading.Event()
        event.set()

        engine.submit_message('expensive prompt ' * 100, cancel_event=event)

        assert engine.total_usage == initial_usage, (
            'cancelled turn must not increment token counters'
        )

    def test_pre_set_event_preserves_permission_denials(self) -> None:
        engine = _fresh_engine()
        event = threading.Event()
        event.set()

        denials = (PermissionDenial(tool_name='BashTool', reason='destructive'),)
        engine.submit_message('run bash ls', denied_tools=denials, cancel_event=event)

        assert engine.permission_denials == [], (
            'cancelled turn must not extend permission_denials'
        )


class TestCancellationAfterBudgetCheck:
    """Event set between budget projection and commit => 'cancelled', state intact.

    This simulates the realistic racy case: engine starts computing output,
    caller hits deadline, sets event. Engine observes at post-budget checkpoint
    and returns cleanly.
    """

    def test_post_budget_cancel_returns_cancelled(self) -> None:
        engine = _fresh_engine()
        event = threading.Event()

        # Patch: set the event after projection but before mutation. We do this
        # by wrapping _format_output (called mid-submit) to set the event.
        original_format = engine._format_output

        def _set_then_format(*args, **kwargs):
            result = original_format(*args, **kwargs)
            event.set()  # trigger cancel right after output is built
            return result

        engine._format_output = _set_then_format  # type: ignore[method-assign]

        result = engine.submit_message('hello', cancel_event=event)

        assert result.stop_reason == 'cancelled'
        # Output IS built here (we're past the pre-budget checkpoint), so it's
        # not empty. The contract is about *state*, not output synthesis.
        assert result.output != ''
        # Critical: state still unchanged
        assert engine.mutable_messages == []
        assert engine.transcript_store.entries == []


class TestCancellationAfterCommit:
    """Event set after commit is not observable \u2014 honest cooperative limit."""

    def test_post_commit_cancel_is_not_observable(self) -> None:
        engine = _fresh_engine()
        event = threading.Event()

        # Event only set *after* submit_message returns. The first call has
        # already committed before the event is set.
        result = engine.submit_message('hello', cancel_event=event)
        event.set()  # too late

        assert result.stop_reason == 'completed', (
            'cancel set after commit must not retroactively invalidate the turn'
        )
        assert engine.mutable_messages == ['hello']

    def test_next_call_observes_cancel(self) -> None:
        """The cancel_event persists \u2014 the next call on the same engine sees it."""
        engine = _fresh_engine()
        event = threading.Event()

        engine.submit_message('first', cancel_event=event)
        assert engine.mutable_messages == ['first']

        event.set()
        # Next call observes the cancel at entry
        result = engine.submit_message('second', cancel_event=event)

        assert result.stop_reason == 'cancelled'
        # 'second' must NOT have been committed
        assert engine.mutable_messages == ['first']


class TestLegacyCallersUnchanged:
    """cancel_event=None (default) => zero behaviour change from pre-#164."""

    def test_no_event_submits_normally(self) -> None:
        engine = _fresh_engine()
        result = engine.submit_message('hello')

        assert result.stop_reason == 'completed'
        assert engine.mutable_messages == ['hello']

    def test_no_event_with_budget_overflow_still_rejects_atomically(self) -> None:
        """#162 atomicity contract survives when cancel_event is absent."""
        engine = _fresh_engine(max_budget_tokens=1)
        words = ' '.join(['word'] * 100)

        result = engine.submit_message(words)  # no cancel_event

        assert result.stop_reason == 'max_budget_reached'
        assert engine.mutable_messages == []

    def test_no_event_respects_max_turns(self) -> None:
        """max_turns_reached contract survives when cancel_event is absent."""
        engine = _fresh_engine(max_turns=1)
        engine.submit_message('first')
        result = engine.submit_message('second')  # no cancel_event

        assert result.stop_reason == 'max_turns_reached'
        assert engine.mutable_messages == ['first']


class TestCancellationVsOtherStopReasons:
    """cancel_event has a defined precedence relative to budget/turns."""

    def test_cancel_precedes_max_turns_check(self) -> None:
        """If cancel is set when capacity is also full, cancel wins (clearer signal)."""
        engine = _fresh_engine(max_turns=0)  # immediately full
        event = threading.Event()
        event.set()

        result = engine.submit_message('hello', cancel_event=event)

        # cancel_event check is the very first thing in submit_message,
        # so it fires before the max_turns check even sees capacity
        assert result.stop_reason == 'cancelled'

    def test_cancel_does_not_override_commit(self) -> None:
        """Completed turn with late cancel still reports 'completed' \u2014 the
        turn already succeeded; we don't lie about it."""
        engine = _fresh_engine()
        event = threading.Event()

        # Event gets set after the mutation is done \u2014 submit_message doesn't
        # re-check after commit
        result = engine.submit_message('hello', cancel_event=event)
        event.set()

        assert result.stop_reason == 'completed'
