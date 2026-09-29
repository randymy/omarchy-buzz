import unittest
from unittest.mock import patch
from pathlib import Path
import claude_policy_native_probe as probe


class NativeLoginSyntaxTests(unittest.TestCase):
    def test_native_version_and_subscription_flag(self):
        self.assertEqual(probe.classify('2.1.0 (Claude Code)\n',
            'Usage: claude auth login [options]\n  --claudeai  Log in with Claude\n'),
            {'nativeVersion': '2.1.0', 'subscriptionLoginFlagSupported': True})

    def test_rejects_ambiguous_or_unrelated_output(self):
        for version, help_text in (
            ('unexpected private payload', 'Usage: claude auth login\n --claudeai'),
            ('2.1.0 (Claude Code)', 'Example: --claudeai'),
            ('2.1.0 (Claude Code)', 'Usage: claude auth login\n --claudeai-other'),
        ):
            with self.subTest(version=version), self.assertRaises(probe.ProbeFailure):
                probe.classify(version, help_text)

    def test_only_help_and_version_can_run(self):
        outputs = [b'2.1.0 (Claude Code)\n', b'Usage: claude auth login [options]\n --claudeai  Login\n']
        with patch.object(probe, 'capture_bounded', side_effect=outputs) as capture:
            probe.run_probe(Path('/usr/bin/node'), Path('/fixture/dist/index.js'))
        self.assertEqual([c.args[0][2:] for c in capture.call_args_list],
                         [['--cli', '--version'], ['--cli', 'auth', 'login', '--help']])
        for call in capture.call_args_list:
            self.assertNotIn('ANTHROPIC_API_KEY', call.args[1])
            self.assertIn('CLAUDE_CONFIG_DIR', call.args[1])


if __name__ == '__main__':
    unittest.main()
