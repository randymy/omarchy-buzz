"""Protocol and redaction tests using a synthetic app-server peer only."""

from pathlib import Path
import sys
import tempfile
import time
import unittest

import codex_subscription_probe as probe


class SubscriptionProbeTests(unittest.TestCase):
    def test_exact_rejection_required(self):
        probe._require_response({"id": 1, "result": {}}, 1)
        probe._require_response({"id": 2, "error": {
            "code": -32600, "message": probe.EXPECTED_ERROR_MESSAGE,
        }}, 2)
        for response in (
            {"id": 2, "result": {}},
            {"id": 2, "error": {"code": -32602, "message": probe.EXPECTED_ERROR_MESSAGE}},
            {"id": 2, "error": {"code": -32600, "message": "other"}},
        ):
            with self.subTest(response=response), self.assertRaises(probe.ProbeFailure):
                probe._require_response(response, 2)

    def test_synthetic_peer_sequence_and_bounded_capture(self):
        fixture = '''import json, sys
first = json.loads(sys.stdin.readline())
assert first["method"] == "initialize" and first["id"] == 1
print(json.dumps({"id": 1, "result": {}}), flush=True)
assert json.loads(sys.stdin.readline())["method"] == "initialized"
second = json.loads(sys.stdin.readline())
assert second == {"id": 2, "method": "account/login/start", "params": {"type": "apiKey", "apiKey": "sk-test-key"}}
print(json.dumps({"id": 2, "error": {"code": -32600, "message": "API key login is disabled. Use ChatGPT login instead."}}), flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "peer.py"
            entry.write_text(fixture)
            probe.run_native_probe(Path(sys.executable), entry, time.monotonic() + 5)

    def test_synthetic_peer_unexpected_success_fails_closed(self):
        fixture = '''import json, sys
sys.stdin.readline()
print(json.dumps({"id": 1, "result": {}}), flush=True)
sys.stdin.readline()
sys.stdin.readline()
print(json.dumps({"id": 2, "result": {}}), flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "peer.py"
            entry.write_text(fixture)
            with self.assertRaisesRegex(probe.ProbeFailure, "api_login_not_rejected"):
                probe.run_native_probe(Path(sys.executable), entry, time.monotonic() + 5)

    def test_synthetic_peer_output_cap(self):
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "peer.py"
            entry.write_text('import sys\nsys.stdin.readline()\nsys.stderr.write("x" * 70000)\nsys.stderr.flush()\n')
            with self.assertRaisesRegex(probe.ProbeFailure, "native_output_limit"):
                probe.run_native_probe(Path(sys.executable), entry, time.monotonic() + 5)


if __name__ == "__main__":
    unittest.main()
