"""#178 — argparse-level errors emit JSON envelope when --output-format json is requested.

Before #178:
  $ claw nonexistent --output-format json
  usage: main.py [-h] {summary,manifest,...} ...
  main.py: error: argument command: invalid choice: 'nonexistent' (choose from ...)
  [exit 2, argparse dumps help to stderr, no JSON envelope]

After #178:
  $ claw nonexistent --output-format json
  {"timestamp": "...", "command": "nonexistent", "exit_code": 1, ...,
   "error": {"kind": "parse", "operation": "argparse", ...}}
  [exit 1, JSON envelope on stdout, matches SCHEMAS.md contract]

Contract:
- text mode: unchanged (argparse still dumps help to stderr, exit code 2)
- JSON mode: envelope matches SCHEMAS.md 'error' shape, exit code 1
- Parse errors use error.kind='parse' (distinct from runtime/session/etc.)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

CLI = [sys.executable, '-m', 'src.main']
REPO_ROOT = Path(__file__).resolve().parent.parent


class TestParseErrorJsonEnvelope:
    """Argparse errors emit JSON envelope when --output-format json is requested."""

    def test_unknown_command_json_mode_emits_envelope(self) -> None:
        """Unknown command + --output-format json → parse-error envelope."""
        result = subprocess.run(
            CLI + ['nonexistent-command', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1, f"expected exit 1; got {result.returncode}"
        envelope = json.loads(result.stdout)
        # Common fields
        assert envelope['schema_version'] == '1.0'
        assert envelope['output_format'] == 'json'
        assert envelope['exit_code'] == 1
        # Error envelope shape
        assert envelope['error']['kind'] == 'parse'
        assert envelope['error']['operation'] == 'argparse'
        assert envelope['error']['retryable'] is False
        assert envelope['error']['target'] == 'nonexistent-command'
        assert 'hint' in envelope['error']

    def test_unknown_command_json_equals_syntax(self) -> None:
        """--output-format=json syntax also works."""
        result = subprocess.run(
            CLI + ['nonexistent-command', '--output-format=json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 1
        envelope = json.loads(result.stdout)
        assert envelope['error']['kind'] == 'parse'

    def test_unknown_command_text_mode_unchanged(self) -> None:
        """Text mode (default) preserves argparse behavior: help to stderr, exit 2."""
        result = subprocess.run(
            CLI + ['nonexistent-command'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 2, f"text mode must preserve argparse exit 2; got {result.returncode}"
        # stderr should have argparse error (help + error message)
        assert 'invalid choice' in result.stderr
        # stdout should be empty (no JSON leaked)
        assert result.stdout == ''

    def test_invalid_flag_json_mode_emits_envelope(self) -> None:
        """Invalid flag at top level + --output-format json → envelope."""
        result = subprocess.run(
            CLI + ['--invalid-top-level-flag', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        # argparse might reject before --output-format is parsed; still emit envelope
        assert result.returncode == 1, f"got {result.returncode}: {result.stderr}"
        envelope = json.loads(result.stdout)
        assert envelope['error']['kind'] == 'parse'

    def test_missing_command_no_json_flag_behaves_normally(self) -> None:
        """No --output-format flag + missing command → normal argparse behavior."""
        result = subprocess.run(
            CLI,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        # argparse exits 2 when required subcommand is missing
        assert result.returncode == 2
        assert 'required' in result.stderr.lower() or 'the following arguments are required' in result.stderr.lower()

    def test_valid_command_unaffected(self) -> None:
        """Valid commands still work normally (no regression)."""
        result = subprocess.run(
            CLI + ['list-sessions', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        envelope = json.loads(result.stdout)
        assert envelope['command'] == 'list-sessions'
        assert 'sessions' in envelope

    def test_parse_error_envelope_contains_common_fields(self) -> None:
        """Parse-error envelope must include all common fields per SCHEMAS.md."""
        result = subprocess.run(
            CLI + ['bogus', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        envelope = json.loads(result.stdout)
        # All common fields required by SCHEMAS.md
        for field in ('timestamp', 'command', 'exit_code', 'output_format', 'schema_version'):
            assert field in envelope, f"common field '{field}' missing from parse-error envelope"


class TestParseErrorSchemaCompliance:
    """Parse-error envelope matches SCHEMAS.md error shape."""

    def test_error_kind_is_parse(self) -> None:
        """error.kind='parse' distinguishes argparse errors from runtime errors."""
        result = subprocess.run(
            CLI + ['unknown', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        envelope = json.loads(result.stdout)
        assert envelope['error']['kind'] == 'parse'

    def test_error_retryable_false(self) -> None:
        """Parse errors are never retryable (typo won't magically fix itself)."""
        result = subprocess.run(
            CLI + ['unknown', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        envelope = json.loads(result.stdout)
        assert envelope['error']['retryable'] is False
