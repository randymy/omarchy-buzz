import unittest
from unittest.mock import patch
from pathlib import Path
import guarded_adapter_harness_probe as probe


class GuardedHarnessTests(unittest.TestCase):
    def test_only_subscription_methods_allowed(self):
        probe.require_methods('claude', False, [])
        probe.require_methods('claude', True, [{'id': 'claude-ai-login', 'type': 'terminal'}])
        probe.require_methods('codex', True, [{'id': 'chat-gpt', 'type': 'agent'}])
        for vendor, terminal, methods in (
            ('claude', False, [{'id': 'claude-ai-login', 'type': 'terminal'}]),
            ('claude', True, [{'id': 'console-login', 'type': 'terminal'}]),
            ('codex', True, [{'id': 'api-key', 'type': 'agent'}]),
            ('codex', False, []),
        ):
            with self.subTest(vendor=vendor, methods=methods), self.assertRaises(probe.ProbeFailure):
                probe.require_methods(vendor, terminal, methods)

    def test_discovery_only_and_scoped_profile(self):
        for vendor in ('claude', 'codex'):
            output = b'{"methods":[{"id":"claude-ai-login","type":"terminal"}]}' if vendor == 'claude' else b'{"methods":[{"id":"chat-gpt","type":"agent"}]}'
            with patch.object(probe, 'capture_bounded', return_value=output) as capture:
                probe.run_case(Path('/fixture/buzz-acp'), Path('/usr/bin/node'),
                               Path('/fixture/dist/index.js'), vendor, True)
            argv, env, cwd, _ = capture.call_args.args
            self.assertEqual(argv[1], 'auth-methods')
            self.assertIn('--terminal-auth', argv)
            self.assertIn(probe.VENDORS[vendor][5], argv[5])
            self.assertEqual(Path(env[probe.VENDORS[vendor][4]]).parent, cwd)
            self.assertNotIn('OPENAI_API_KEY', env)
            self.assertNotIn('ANTHROPIC_API_KEY', env)


if __name__ == '__main__':
    unittest.main()
