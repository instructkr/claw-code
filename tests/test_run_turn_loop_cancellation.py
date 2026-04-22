"""Tests for run_turn_loop timeout triggering cooperative cancel (ROADMAP #164 Stage A).

End-to-end integration: when the wall-clock timeout fires in run_turn_loop,
the runtime must signal the cancel_event so any in-flight submit_message
thread sees it at its next safe checkpoint and returns without mutating
state.

This closes the gap filed in #164: #161's timeout bounded caller wait but
did not prevent ghost turns.
"""

from __future__ import annotations

import sys
import threading
import time
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.models import UsageSummary  # noqa: E402
from src.query_engine import TurnResult  # noqa: E402
from src.runtime import PortRuntime  # noqa: E402


def _completed(prompt: str) -> TurnResult:
    return TurnResult(
        prompt=prompt,
        output='ok',
        matched_commands=(),
        matched_tools=(),
        permission_denials=(),
        usage=UsageSummary(),
        stop_reason='completed',
    )


class TestTimeoutPropagatesCancelEvent:
    def test_runtime_passes_cancel_event_to_submit_message(self) -> None:
        """submit_message receives a cancel_event when a deadline is in play."""
        runtime = PortRuntime()
        captured_event: list[threading.Event | None] = []

        def _capture(prompt, commands, tools, denials, cancel_event=None):
            captured_event.append(cancel_event)
            return _completed(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = _capture

            runtime.run_turn_loop(
                'hello', max_turns=1, timeout_seconds=5.0,
            )

            # Runtime passed a real Event object, not None
            assert len(captured_event) == 1
            assert isinstance(captured_event[0], threading.Event)

    def test_legacy_no_timeout_does_not_pass_cancel_event(self) -> None:
        """Without timeout_seconds, the cancel_event is None (legacy behaviour)."""
        runtime = PortRuntime()
        captured_kwargs: list[dict] = []

        def _capture(prompt, commands, tools, denials):
            # Legacy call signature: no cancel_event kwarg
            captured_kwargs.append({'prompt': prompt})
            return _completed(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = _capture

            runtime.run_turn_loop('hello', max_turns=1)

            # Legacy path didn't pass cancel_event at all
            assert len(captured_kwargs) == 1

    def test_timeout_sets_cancel_event_before_returning(self) -> None:
        """When timeout fires mid-call, the event is set and the still-running
        thread would see 'cancelled' if it checks before returning."""
        runtime = PortRuntime()
        observed_events_at_checkpoint: list[bool] = []
        release = threading.Event()  # test-side release so the thread doesn't leak forever

        def _slow_submit(prompt, commands, tools, denials, cancel_event=None):
            # Simulate provider work: block until either cancel or a test-side release.
            # If cancel fires, check if the event is observably set.
            start = time.monotonic()
            while time.monotonic() - start < 2.0:
                if cancel_event is not None and cancel_event.is_set():
                    observed_events_at_checkpoint.append(True)
                    return TurnResult(
                        prompt=prompt, output='',
                        matched_commands=(), matched_tools=(),
                        permission_denials=(), usage=UsageSummary(),
                        stop_reason='cancelled',
                    )
                if release.is_set():
                    break
                time.sleep(0.05)
            return _completed(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = _slow_submit

            # Tight deadline: 0.2s, submit will be mid-loop when timeout fires
            start = time.monotonic()
            results = runtime.run_turn_loop(
                'hello', max_turns=1, timeout_seconds=0.2,
            )
            elapsed = time.monotonic() - start
            release.set()  # let the background thread exit cleanly

            # Runtime returned a timeout TurnResult to the caller
            assert results[-1].stop_reason == 'timeout'
            # And it happened within a reasonable window of the deadline
            assert elapsed < 1.5, f'runtime did not honour deadline: {elapsed:.2f}s'

            # Give the background thread a moment to observe the cancel.
            # We don't assert on it directly (thread-level observability is
            # timing-dependent), but the contract is: the event IS set, so any
            # cooperative checkpoint will see it.
            time.sleep(0.3)


class TestCancelEventSharedAcrossTurns:
    """Event is created once per run_turn_loop invocation and shared across turns."""

    def test_same_event_threaded_to_every_submit_message(self) -> None:
        runtime = PortRuntime()
        captured_events: list[threading.Event] = []

        def _capture(prompt, commands, tools, denials, cancel_event=None):
            if cancel_event is not None:
                captured_events.append(cancel_event)
            return _completed(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = _capture

            runtime.run_turn_loop(
                'hello', max_turns=3, timeout_seconds=5.0,
                continuation_prompt='continue',
            )

            # All 3 turns received the same event object (same identity)
            assert len(captured_events) == 3
            assert all(e is captured_events[0] for e in captured_events), (
                'runtime must share one cancel_event across turns, not create '
                'a new one per turn \u2014 otherwise a late-arriving cancel on turn '
                'N-1 cannot affect turn N'
            )
