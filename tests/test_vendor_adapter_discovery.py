"""Local boundary tests; no vendor adapter or account is used."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import vendor_adapter_discovery as probe


class DiscoveryTests(unittest.TestCase):
    def test_only_method_identifiers_and_types_survive(self):
        raw = json.dumps({'methods': [{'id': 'chat-gpt', 'name': 'private name',
                                      'env': {'SECRET': 'private value'}}],
                          'credentials': 'private value'})
        self.assertEqual(probe.sanitize_methods(raw), [{'id': 'chat-gpt', 'type': 'agent'}])

    def test_rejects_malformed_duplicate_and_unknown_types(self):
        cases = [b'not json', b'[]', b'{"methods":{}}',
                 b'{"methods":[{"id":"x"},{"id":"x"}]}',
                 b'{"methods":[{"id":"x","type":"shell"}]}',
                 b'{"methods":[{"id":"line\\nbreak"}]}']
        for raw in cases:
            with self.subTest(raw=raw), self.assertRaises(probe.ProbeFailure):
                probe.sanitize_methods(raw)

    def test_host_network_refused(self):
        with patch.object(os, 'geteuid', return_value=1000):
            with self.assertRaisesRegex(probe.ProbeFailure, 'network_namespace_not_isolated'):
                probe.network_isolated(os.readlink('/proc/self/ns/net'))

    def test_environment_has_no_inherited_secrets(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {'OPENAI_API_KEY': 'fixture'}):
            env = probe.clean_child_env(Path(tmp), Path('/usr/bin/node'))
            self.assertNotIn('OPENAI_API_KEY', env)
            self.assertNotIn('DBUS_SESSION_BUS_ADDRESS', env)
            self.assertEqual(env['HOME'], tmp)

    def test_capture_limits_and_nonzero_redaction(self):
        with tempfile.TemporaryDirectory() as tmp:
            for code, timeout, category in [
                ('import time; time.sleep(10)', .1, 'probe_timeout'),
                ('print("x"*70000)', 2, 'probe_output_limit'),
                ('import sys; print("private", file=sys.stderr); sys.exit(1)', 2, 'probe_nonzero_exit'),
            ]:
                with self.subTest(category=category), self.assertRaisesRegex(probe.ProbeFailure, '^'+category+'$'):
                    probe.capture_bounded([sys.executable, '-c', code], {'PATH': '/usr/bin:/bin'}, tmp, timeout)


if __name__ == '__main__':
    unittest.main()
