"""Tests for --output-format flag on show-command and show-tool (ROADMAP #167).

Verifies parity with session-lifecycle CLI family (#160/#165/#166):
- show-command and show-tool now accept --output-format {text,json}
- Found case returns success with JSON envelope: {name, found: true, source_hint, responsibility}
- Not-found case returns typed error envelope: {name, found: false, error: {kind, message, retryable}}
- Legacy text output (default) unchanged for backward compat
- Exit code 0 on success, 1 on not-found (matching load-session contract)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))


class TestShowCommandOutputFormat:
    """show-command --output-format {text,json} parity with session-lifecycle family."""

    def test_show_command_found_json(self) -> None:
        """show-command with found entry returns JSON envelope."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'add-dir', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, f'Expected exit 0, got {result.returncode}: {result.stderr}'

        envelope = json.loads(result.stdout)
        assert envelope['found'] is True
        assert envelope['name'] == 'add-dir'
        assert 'source_hint' in envelope
        assert 'responsibility' in envelope
        # No error field when found
        assert 'error' not in envelope

    def test_show_command_not_found_json(self) -> None:
        """show-command with missing entry returns typed error envelope."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'nonexistent-cmd', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1, f'Expected exit 1 on not-found, got {result.returncode}'

        envelope = json.loads(result.stdout)
        assert envelope['found'] is False
        assert envelope['name'] == 'nonexistent-cmd'
        assert envelope['error']['kind'] == 'command_not_found'
        assert envelope['error']['retryable'] is False
        # No source_hint/responsibility when not found
        assert 'source_hint' not in envelope or envelope.get('source_hint') is None
        assert 'responsibility' not in envelope or envelope.get('responsibility') is None

    def test_show_command_text_mode_backward_compat(self) -> None:
        """show-command text mode (default) is unchanged from pre-#167."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'add-dir'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0

        # Text output is newline-separated (name, source_hint, responsibility)
        lines = result.stdout.strip().split('\n')
        assert len(lines) == 3
        assert lines[0] == 'add-dir'
        assert 'commands/add-dir/add-dir.tsx' in lines[1]

    def test_show_command_text_mode_not_found(self) -> None:
        """show-command text mode on not-found returns prose error."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'missing'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1
        assert 'not found' in result.stdout.lower()
        assert 'missing' in result.stdout

    def test_show_command_default_is_text(self) -> None:
        """Omitting --output-format defaults to text."""
        result_implicit = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'add-dir'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        result_explicit = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'add-dir', '--output-format', 'text'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result_implicit.stdout == result_explicit.stdout


class TestShowToolOutputFormat:
    """show-tool --output-format {text,json} parity with session-lifecycle family."""

    def test_show_tool_found_json(self) -> None:
        """show-tool with found entry returns JSON envelope."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-tool', 'BashTool', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, f'Expected exit 0, got {result.returncode}: {result.stderr}'

        envelope = json.loads(result.stdout)
        assert envelope['found'] is True
        assert envelope['name'] == 'BashTool'
        assert 'source_hint' in envelope
        assert 'responsibility' in envelope
        assert 'error' not in envelope

    def test_show_tool_not_found_json(self) -> None:
        """show-tool with missing entry returns typed error envelope."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-tool', 'NotARealTool', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1, f'Expected exit 1 on not-found, got {result.returncode}'

        envelope = json.loads(result.stdout)
        assert envelope['found'] is False
        assert envelope['name'] == 'NotARealTool'
        assert envelope['error']['kind'] == 'tool_not_found'
        assert envelope['error']['retryable'] is False

    def test_show_tool_text_mode_backward_compat(self) -> None:
        """show-tool text mode (default) is unchanged from pre-#167."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-tool', 'BashTool'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0

        lines = result.stdout.strip().split('\n')
        assert len(lines) == 3
        assert lines[0] == 'BashTool'
        assert 'tools/BashTool/BashTool.tsx' in lines[1]


class TestShowCommandToolFormatParity:
    """Verify symmetry between show-command and show-tool formats."""

    def test_both_accept_output_format_flag(self) -> None:
        """Both commands accept the same --output-format choices."""
        # Just ensure both fail with invalid choice (they accept text/json)
        result_cmd = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'add-dir', '--output-format', 'invalid'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        result_tool = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-tool', 'BashTool', '--output-format', 'invalid'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        # Both should fail with argument parser error
        assert result_cmd.returncode != 0
        assert result_tool.returncode != 0
        assert 'invalid choice' in result_cmd.stderr
        assert 'invalid choice' in result_tool.stderr

    def test_json_envelope_shape_consistency(self) -> None:
        """Both commands return consistent JSON envelope shape."""
        cmd_result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-command', 'add-dir', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        tool_result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'show-tool', 'BashTool', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )

        cmd_envelope = json.loads(cmd_result.stdout)
        tool_envelope = json.loads(tool_result.stdout)

        # Same top-level keys for found=true case
        assert set(cmd_envelope.keys()) == set(tool_envelope.keys())
        assert cmd_envelope['found'] is True
        assert tool_envelope['found'] is True
