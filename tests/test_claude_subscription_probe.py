"""Offline synthetic CLI checks for the account-free characterization probe."""

import json
from pathlib import Path
import sys
import tempfile
import time
import unittest

from claude_subscription_probe import (
    ProbeFailure, classify_status, run_case,
)


FAKE_CLI = r'''import json, os, pathlib, sys
assert sys.argv[1:] == ["--cli", "auth", "status", "--json"]
assert "CLAUDE_CODE_OAUTH_TOKEN" not in os.environ
assert "CLAUDE_CODE_EXECUTABLE" not in os.environ
assert "ANTHROPIC_AUTH_TOKEN" not in os.environ
assert "CI_SECRET" not in os.environ
assert os.environ["HOME"] == os.environ["TMPDIR"]
home = pathlib.Path(os.environ["HOME"])
forced = home / ".claude" / "settings.json"
key = os.environ.get("ANTHROPIC_API_KEY")
if key:
    assert key.startswith("sk-ant-api03-")
    assert len(key) > 80
if forced.exists():
    assert json.loads(forced.read_text()) == {"forceLoginMethod": "claudeai"}
    assert key
else:
    assert not (home / ".claude").exists()
print(json.dumps({
    "loggedIn": bool(key),
    "authMethod": "none",
    "apiProvider": "firstParty",
    "apiKeySource": "ANTHROPIC_API_KEY" if key else None,
    "email": "MUST_NOT_APPEAR_IN_RESULT@example.invalid",
}))
sys.exit(0 if key else 1)
'''


class ClaudeSubscriptionProbeTests(unittest.TestCase):
    def fake_cli(self, directory, source):
        script = Path(directory) / "fake_cli.py"
        script.write_text(source)
        return script

    def test_three_cases_are_isolated_and_report_only_labels(self):
        with tempfile.TemporaryDirectory() as temporary:
            script = self.fake_cli(temporary, FAKE_CLI)
            results = {
                case: run_case(Path(sys.executable), script, case, time.monotonic() + 4)
                for case in ("empty", "apiKey", "apiKeyForcedClaudeAi")
            }
        self.assertEqual(results["empty"], {
            "loggedIn": False, "exitClass": "one", "authMethod": "none",
            "apiProvider": "firstParty", "apiKeySource": None,
        })
        for case in ("apiKey", "apiKeyForcedClaudeAi"):
            self.assertEqual(results[case]["apiKeySource"], "ANTHROPIC_API_KEY")
            self.assertTrue(results[case]["loggedIn"])
            self.assertEqual(results[case]["exitClass"], "zero")
        self.assertNotIn("MUST_NOT_APPEAR", json.dumps(results))

    def test_timeout_terminates_probe(self):
        with tempfile.TemporaryDirectory() as temporary:
            script = self.fake_cli(temporary, "import time\ntime.sleep(3)\n")
            with self.assertRaises(ProbeFailure) as caught:
                run_case(Path(sys.executable), script, "empty", time.monotonic() + 0.05)
        self.assertEqual(caught.exception.category, "native_timeout")

    def test_output_is_bounded_and_stderr_not_reported(self):
        with tempfile.TemporaryDirectory() as temporary:
            script = self.fake_cli(temporary, "import sys\nsys.stderr.write('X'*20000)\n")
            with self.assertRaises(ProbeFailure) as caught:
                run_case(Path(sys.executable), script, "empty", time.monotonic() + 3)
        self.assertEqual(caught.exception.category, "native_output_limit")

    def test_unknown_status_and_bad_exit_fail_closed(self):
        with self.assertRaises(ProbeFailure) as caught:
            classify_status(b'{"loggedIn":true,"apiProvider":"newBackend"}', "zero")
        self.assertEqual(caught.exception.category, "status_field_unrecognized")
        with tempfile.TemporaryDirectory() as temporary:
            script = self.fake_cli(temporary, "raise SystemExit(2)\n")
            with self.assertRaises(ProbeFailure) as caught:
                run_case(Path(sys.executable), script, "empty", time.monotonic() + 3)
        self.assertEqual(caught.exception.category, "native_exit_unrecognized")


if __name__ == "__main__":
    unittest.main()
