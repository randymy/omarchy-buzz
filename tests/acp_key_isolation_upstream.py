#!/usr/bin/env python3
"""Synthetic FD-input rejection tests against a complete staged Buzz ACP binary.

Every case stops at configuration validation, before any network or agent spawn.
Only public disposable test scalars are used. No user profile is read.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

BINARY = None
KEY = b'0' * 63 + b'2'


class KeyInputTests(unittest.TestCase):
    def probe(self, data=KEY, *, extra=(), env_extra=None, eof=True, descriptor=None):
        with tempfile.TemporaryDirectory(prefix='buzz-key-fixture-') as temporary:
            root = Path(temporary)
            env = {'HOME': temporary, 'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8',
                   'RUST_LOG': 'off', 'XDG_CONFIG_HOME': str(root/'config'),
                   'XDG_DATA_HOME': str(root/'data'), 'XDG_CACHE_HOME': str(root/'cache'),
                   'TMPDIR': temporary}
            env.update(env_extra or {})
            reader = writer = None
            try:
                if descriptor is None:
                    reader, writer = os.pipe2(os.O_CLOEXEC)
                    os.write(writer, data)
                    if eof:
                        os.close(writer)
                        writer = None
                    fd = reader
                    passed = (reader,)
                else:
                    fd = descriptor
                    passed = ()
                argv = [str(BINARY), '--private-key-fd', str(fd), '--agent-command', '', *extra]
                result = subprocess.run(argv, cwd=root, env=env, pass_fds=passed,
                                        stdin=subprocess.DEVNULL, capture_output=True, timeout=7)
                output = result.stdout + result.stderr
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn(KEY, output, 'fixture secret appeared in diagnostics')
                self.assertLess(len(output), 8192)
                self.assertFalse(list(root.rglob('.nostr-key')))
                return output
            finally:
                for fd in (reader, writer):
                    if fd is not None:
                        os.close(fd)

    def test_valid_pipe_reaches_later_validation(self):
        self.assertIn(b'agent_command must not be empty', self.probe())

    def test_invalid_key_is_redacted(self):
        self.assertIn(b'private_key_fd_invalid_key', self.probe(b'not-a-key'))

    def test_oversized_input_is_rejected(self):
        self.assertIn(b'private_key_fd_too_large', self.probe(b'x'*129))

    def test_stalled_writer_is_bounded(self):
        self.assertIn(b'private_key_fd_timeout', self.probe(eof=False))

    def test_closed_descriptor_is_rejected(self):
        self.assertIn(b'private_key_fd_invalid', self.probe(descriptor=2147483647))

    def test_stdio_is_rejected_by_cli(self):
        self.assertIn(b'invalid value', self.probe(descriptor=0))

    def test_ambient_key_sources_conflict(self):
        for name in ('BUZZ_PRIVATE_KEY', 'BUZZ_ACP_PRIVATE_KEY'):
            with self.subTest(name=name):
                self.assertIn(b'cannot be used with', self.probe(env_extra={name: KEY.decode()}))

    def test_secret_dependent_features_are_rejected(self):
        for extra in (('--mcp-command', 'forbidden-fixture'), ('--base-prompt-file', '/nonexistent-fixture')):
            with self.subTest(extra=extra):
                self.assertIn(b'cannot share relay credentials', self.probe(extra=extra))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    BINARY = args.binary.resolve(strict=True)
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(KeyInputTests))
    summary = {'successful': result.wasSuccessful(), 'testsRun': result.testsRun,
               'failures': len(result.failures), 'errors': len(result.errors),
               'syntheticKeysOnly': True, 'relayUsed': False, 'agentStarted': False}
    args.output.write_text(json.dumps(summary, sort_keys=True)+'\n')
    raise SystemExit(0 if result.wasSuccessful() else 1)
