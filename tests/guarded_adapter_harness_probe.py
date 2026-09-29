#!/usr/bin/env python3
"""Discover real guarded adapters through compiled Buzz ACP, without login/turns."""
import argparse
import json
import os
from pathlib import Path
import tempfile

from vendor_adapter_discovery import (
    ProbeFailure, capture_bounded, clean_child_env, installed_version,
    network_isolated, sanitize_methods,
)

VENDORS = {
    'codex': ('@agentclientprotocol/codex-acp', '2.0.0', '@openai/codex', '0.158.0',
              'CODEX_HOME', '--require-chatgpt-subscription'),
    'claude': ('@agentclientprotocol/claude-agent-acp', '0.82.0',
               '@anthropic-ai/claude-agent-sdk', '0.3.280',
               'CLAUDE_CONFIG_DIR', '--require-claude-subscription'),
}


def require_methods(vendor, terminal, methods):
    by_id = {item['id']: item['type'] for item in methods}
    if vendor == 'claude':
        expected = {'claude-ai-login': 'terminal'} if terminal else {}
        if by_id != expected:
            raise ProbeFailure('guarded_claude_methods_mismatch')
    elif vendor == 'codex':
        if (by_id.get('chat-gpt') != 'agent' or
                any(key not in ('chat-gpt', 'chat-gpt-device-code') or value != 'agent'
                    for key, value in by_id.items())):
            raise ProbeFailure('guarded_codex_methods_mismatch')
    else:
        raise ProbeFailure('vendor_invalid')


def run_case(binary, node, adapter, vendor, terminal):
    if vendor not in VENDORS or ',' in str(adapter):
        raise ProbeFailure('input_invalid')
    with tempfile.TemporaryDirectory(prefix='guarded-harness-') as temporary:
        home = Path(temporary)
        env = clean_child_env(home, node)
        profile = home / 'provider-profile'
        profile.mkdir(mode=0o700)
        env[VENDORS[vendor][4]] = str(profile)
        argv = [str(binary), 'auth-methods', '--agent-command', str(node),
                '--agent-args', str(adapter) + ',' + VENDORS[vendor][5], '--json']
        if terminal:
            argv.append('--terminal-auth')
        methods = sanitize_methods(capture_bounded(argv, env, home, 35))
    require_methods(vendor, terminal, methods)
    return methods


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--codex-source', type=Path, required=True)
    parser.add_argument('--claude-source', type=Path, required=True)
    parser.add_argument('--outside-netns', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'successful': False, 'networkIsolated': False, 'realAccountUsed': False,
              'loginStarted': False, 'sessionCreated': False, 'modelTurnRequested': False,
              'methods': {}, 'failures': []}
    try:
        report['networkIsolated'] = network_isolated(args.outside_netns)
        for path in (args.binary, args.node, args.codex_source, args.claude_source):
            if not path.is_absolute():
                raise ProbeFailure('input_path_not_absolute')
        binary, node = args.binary.resolve(strict=True), args.node.resolve(strict=True)
        if not all(p.is_file() and os.access(p, os.X_OK) for p in (binary, node)):
            raise ProbeFailure('binary_unavailable')
        for vendor, source in (('codex', args.codex_source), ('claude', args.claude_source)):
            source = source.resolve(strict=True)
            package, version, native, native_version, _, _ = VENDORS[vendor]
            metadata = json.loads((source / 'package.json').read_text())
            if metadata.get('name') != package or metadata.get('version') != version:
                raise ProbeFailure('adapter_identity_mismatch')
            if installed_version(source, native) != native_version:
                raise ProbeFailure('native_version_mismatch')
            adapter = (source / 'dist/index.js').resolve(strict=True)
            if not adapter.is_file() or not adapter.is_relative_to(source):
                raise ProbeFailure('adapter_entry_invalid')
            report['methods'][vendor] = {}
            for terminal in (False, True):
                label = 'terminalOptIn' if terminal else 'default'
                report['methods'][vendor][label] = run_case(binary, node, adapter, vendor, terminal)
        report['successful'] = True
    except ProbeFailure as error:
        report['failures'].append(error.category)
    except (OSError, ValueError, TypeError, AttributeError):
        report['failures'].append('probe_setup_failed')
    encoded = json.dumps(report, sort_keys=True) + '\n'
    args.output.write_text(encoded)
    print(encoded, end='')
    return 0 if report['successful'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
