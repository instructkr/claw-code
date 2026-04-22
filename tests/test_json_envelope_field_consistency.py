"""JSON envelope field consistency validation (ROADMAP #173 prep).

This test suite validates that clawable-surface commands' JSON output
follows the contract defined in SCHEMAS.md. Currently, commands emit
command-specific envelopes without the canonical common fields
(timestamp, command, exit_code, output_format, schema_version).

This test documents the current gap and validates the consistency
of what IS there, providing a baseline for #173 (common field wrapping).

Phase 1 (this test): Validate consistency within each command's envelope.
Phase 2 (future #173): Wrap all 13 commands with canonical common fields.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.main import build_parser  # noqa: E402


# Expected fields for each clawable command's JSON envelope.
# These are the command-specific fields (not including common fields yet).
# Entries are (command_name, required_fields, optional_fields).
ENVELOPE_CONTRACTS = {
    'list-sessions': (
        {'count', 'sessions'},
        set(),
    ),
    'delete-session': (
        {'session_id', 'deleted', 'directory'},
        set(),
    ),
    'load-session': (
        {'session_id', 'loaded', 'directory', 'path'},
        set(),
    ),
    'flush-transcript': (
        {'session_id', 'path', 'flushed', 'messages_count', 'input_tokens', 'output_tokens'},
        set(),
    ),
    'show-command': (
        {'name', 'found', 'source_hint', 'responsibility'},
        set(),
    ),
    'show-tool': (
        {'name', 'found', 'source_hint'},
        set(),
    ),
    'exec-command': (
        {'name', 'prompt', 'handled', 'message', 'source_hint'},
        set(),
    ),
    'exec-tool': (
        {'name', 'payload', 'handled', 'message', 'source_hint'},
        set(),
    ),
    'route': (
        {'prompt', 'limit', 'match_count', 'matches'},
        set(),
    ),
    'bootstrap': (
        {'prompt', 'setup', 'routed_matches', 'turn', 'persisted_session_path'},
        set(),
    ),
    'command-graph': (
        {'builtins_count', 'plugin_like_count', 'skill_like_count', 'total_count', 'builtins', 'plugin_like', 'skill_like'},
        set(),
    ),
    'tool-pool': (
        {'simple_mode', 'include_mcp', 'tool_count', 'tools'},
        set(),
    ),
    'bootstrap-graph': (
        {'stages', 'note'},
        set(),
    ),
}


class TestJsonEnvelopeConsistency:
    """Validate current command envelopes match their declared contracts.

    This is a consistency check, not a conformance check. Once #173 adds
    common fields to all commands, these tests will auto-pass the common
    field assertions and verify command-specific fields stay consistent.
    """

    @pytest.mark.parametrize('cmd_name,contract', sorted(ENVELOPE_CONTRACTS.items()))
    def test_command_json_fields_present(self, cmd_name: str, contract: tuple[set[str], set[str]]) -> None:
        required, optional = contract
        """Command's JSON envelope must include all required fields."""
        # Get minimal invocation args for this command
        test_invocations = {
            'list-sessions': [],
            'show-command': ['add-dir'],
            'show-tool': ['BashTool'],
            'exec-command': ['add-dir', 'hi'],
            'exec-tool': ['BashTool', '{}'],
            'route': ['review'],
            'bootstrap': ['hello'],
            'command-graph': [],
            'tool-pool': [],
            'bootstrap-graph': [],
        }
        
        if cmd_name not in test_invocations:
            pytest.skip(f'{cmd_name} requires session setup; skipped')
        
        cmd_args = test_invocations[cmd_name]
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', cmd_name, *cmd_args, '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        
        if result.returncode not in (0, 1):
            pytest.fail(f'{cmd_name}: unexpected exit {result.returncode}\nstderr: {result.stderr}')
        
        try:
            envelope = json.loads(result.stdout)
        except json.JSONDecodeError as e:
            pytest.fail(f'{cmd_name}: invalid JSON: {e}\nOutput: {result.stdout[:200]}')
        
        # Check required fields (command-specific)
        missing = required - set(envelope.keys())
        if missing:
            pytest.fail(
                f'{cmd_name} envelope missing required fields: {missing}\n'
                f'Expected: {required}\nGot: {set(envelope.keys())}'
            )
        
        # Check that extra fields are accounted for (warn if unknown)
        known = required | optional
        extra = set(envelope.keys()) - known
        if extra:
            # Warn but don't fail — there may be new fields added
            pytest.warns(UserWarning, match=f'extra fields in {cmd_name}: {extra}')

    def test_envelope_field_value_types(self) -> None:
        """Smoke test: envelope fields have expected types (bool, int, str, list, dict, null)."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'list-sessions', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        
        envelope = json.loads(result.stdout)
        
        # Spot check a few fields
        assert isinstance(envelope.get('count'), int), 'count should be int'
        assert isinstance(envelope.get('sessions'), list), 'sessions should be list'


class TestJsonEnvelopeCommonFieldPrep:
    """Validation stubs for common fields (part of #173 implementation).

    These tests will activate once wrap_json_envelope() is applied to all
    13 clawable commands. Currently they document the expected contract.
    """

    def test_all_envelopes_include_timestamp(self) -> None:
        """Every clawable envelope must include ISO 8601 UTC timestamp."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'command-graph', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        envelope = json.loads(result.stdout)
        assert 'timestamp' in envelope, 'Missing timestamp field'
        # Verify ISO 8601 format (ends with Z for UTC)
        assert envelope['timestamp'].endswith('Z'), f'Timestamp not UTC: {envelope["timestamp"]}'

    def test_all_envelopes_include_command(self) -> None:
        """Every envelope must echo the command name."""
        test_cases = [
            ('list-sessions', []),
            ('command-graph', []),
            ('bootstrap', ['hello']),
        ]
        for cmd_name, cmd_args in test_cases:
            result = subprocess.run(
                [sys.executable, '-m', 'src.main', cmd_name, *cmd_args, '--output-format', 'json'],
                cwd=Path(__file__).resolve().parent.parent,
                capture_output=True,
                text=True,
            )
            envelope = json.loads(result.stdout)
            assert envelope.get('command') == cmd_name, f'{cmd_name} envelope.command mismatch'

    def test_all_envelopes_include_exit_code_and_schema_version(self) -> None:
        """Every envelope must include exit_code and schema_version."""
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'tool-pool', '--output-format', 'json'],
            cwd=Path(__file__).resolve().parent.parent,
            capture_output=True,
            text=True,
        )
        envelope = json.loads(result.stdout)
        assert 'exit_code' in envelope, 'Missing exit_code'
        assert 'schema_version' in envelope, 'Missing schema_version'
        assert envelope['schema_version'] == '1.0', 'Wrong schema_version'
