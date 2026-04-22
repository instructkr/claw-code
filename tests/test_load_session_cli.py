"""Tests for load-session CLI parity with list-sessions/delete-session (ROADMAP #165).

Verifies the session-lifecycle CLI triplet is now symmetric:
- --directory DIR accepted (alternate storage locations reachable)
- --output-format {text,json} accepted
- Not-found emits typed JSON error envelope, never a Python traceback
- Corrupted session file distinguished from not-found via 'kind'
- Legacy text-mode output unchanged (backward compat)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.session_store import StoredSession, save_session  # noqa: E402


_REPO_ROOT = Path(__file__).resolve().parent.parent


def _run_cli(
    *args: str, cwd: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    """Always invoke the CLI with cwd=repo-root so ``python -m src.main``
    can resolve the ``src`` package, regardless of where the test's
    tmp_path is.
    """
    return subprocess.run(
        [sys.executable, '-m', 'src.main', *args],
        capture_output=True,
        text=True,
        cwd=str(cwd) if cwd else str(_REPO_ROOT),
    )


def _make_session(session_id: str) -> StoredSession:
    return StoredSession(
        session_id=session_id, messages=('hi',), input_tokens=1, output_tokens=2,
    )


class TestDirectoryFlagParity:
    def test_load_session_accepts_directory_flag(self, tmp_path: Path) -> None:
        save_session(_make_session('alpha'), tmp_path)
        result = _run_cli('load-session', 'alpha', '--directory', str(tmp_path))
        assert result.returncode == 0, result.stderr
        assert 'alpha' in result.stdout

    def test_load_session_without_directory_uses_cwd_default(
        self, tmp_path: Path,
    ) -> None:
        """When --directory is omitted, fall back to .port_sessions in CWD.

        Subprocess CWD must still be able to import ``src.main``, so we use
        ``cwd=tmp_path`` which means ``python -m src.main`` needs ``src/`` on
        sys.path. We set PYTHONPATH to the repo root via env.
        """
        sessions_dir = tmp_path / '.port_sessions'
        sessions_dir.mkdir()
        save_session(_make_session('beta'), sessions_dir)
        import os
        env = os.environ.copy()
        env['PYTHONPATH'] = str(_REPO_ROOT)
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'load-session', 'beta'],
            capture_output=True, text=True, cwd=str(tmp_path), env=env,
        )
        assert result.returncode == 0, result.stderr
        assert 'beta' in result.stdout


class TestOutputFormatFlagParity:
    def test_json_mode_on_success(self, tmp_path: Path) -> None:
        save_session(
            StoredSession(
                session_id='gamma', messages=('x', 'y'),
                input_tokens=5, output_tokens=7,
            ),
            tmp_path,
        )
        result = _run_cli(
            'load-session', 'gamma',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert data == {
            'session_id': 'gamma',
            'loaded': True,
            'messages_count': 2,
            'input_tokens': 5,
            'output_tokens': 7,
        }

    def test_text_mode_unchanged_on_success(self, tmp_path: Path) -> None:
        """Legacy text output must be byte-identical for backward compat."""
        save_session(_make_session('delta'), tmp_path)
        result = _run_cli('load-session', 'delta', '--directory', str(tmp_path))
        assert result.returncode == 0
        lines = result.stdout.strip().split('\n')
        assert lines == ['delta', '1 messages', 'in=1 out=2']


class TestNotFoundTypedError:
    def test_not_found_json_envelope(self, tmp_path: Path) -> None:
        """Not-found emits structured JSON, never a Python traceback."""
        result = _run_cli(
            'load-session', 'missing',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert result.returncode == 1
        assert 'Traceback' not in result.stderr, (
            'regression #165: raw traceback leaked to stderr'
        )
        assert 'SessionNotFoundError' not in result.stdout, (
            'regression #165: internal class name leaked into CLI output'
        )
        data = json.loads(result.stdout)
        assert data['session_id'] == 'missing'
        assert data['loaded'] is False
        assert data['error']['kind'] == 'session_not_found'
        assert data['error']['retryable'] is False
        # directory field is populated so claws know where we looked
        assert 'directory' in data['error']

    def test_not_found_text_mode_no_traceback(self, tmp_path: Path) -> None:
        """Text mode on not-found must not dump a Python stack either."""
        result = _run_cli(
            'load-session', 'missing', '--directory', str(tmp_path),
        )
        assert result.returncode == 1
        assert 'Traceback' not in result.stderr
        assert result.stdout.startswith('error:')


class TestLoadFailedDistinctFromNotFound:
    def test_corrupted_session_file_surfaces_distinct_kind(
        self, tmp_path: Path,
    ) -> None:
        """A corrupted JSON file must emit kind='session_load_failed', not 'session_not_found'."""
        (tmp_path / 'broken.json').write_text('{ not valid json')
        result = _run_cli(
            'load-session', 'broken',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert result.returncode == 1
        data = json.loads(result.stdout)
        assert data['error']['kind'] == 'session_load_failed'
        assert data['error']['retryable'] is True, (
            'corrupted file is potentially retryable (fs glitch) unlike not-found'
        )


class TestTripletParityConsistency:
    """All three #160 CLI commands should accept the same flag pair."""

    @pytest.mark.parametrize('command', ['list-sessions', 'delete-session', 'load-session'])
    def test_all_three_accept_directory_flag(self, command: str) -> None:
        help_text = _run_cli(command, '--help').stdout
        assert '--directory' in help_text, (
            f'{command} missing --directory flag (#165 parity gap)'
        )

    @pytest.mark.parametrize('command', ['list-sessions', 'delete-session', 'load-session'])
    def test_all_three_accept_output_format_flag(self, command: str) -> None:
        help_text = _run_cli(command, '--help').stdout
        assert '--output-format' in help_text, (
            f'{command} missing --output-format flag (#165 parity gap)'
        )
