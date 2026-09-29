"""Synthetic ACP peer checks only; never starts Codex or a provider adapter."""

from pathlib import Path
import sys
import tempfile
import time
import unittest

import codex_adapter_subscription_probe as probe


class AdapterSubscriptionProbeTests(unittest.TestCase):
    def test_exact_policy_rejection_required(self):
        probe.require_response({"jsonrpc":"2.0","id": 1, "result": {"authMethods": [{"id": "api-key"}]}}, 1)
        probe.require_response({"jsonrpc":"2.0","id": 2, "error": {
            "code": -32603, "message": "Internal error", "data": {"details": probe.EXPECTED_ERROR_MESSAGE},
        }}, 2)
        probe.require_response({"jsonrpc":"2.0","id": 2, "error": {
            "code": -32600, "message": probe.EXPECTED_ERROR_MESSAGE,
        }}, 2)
        for response in (
            {"jsonrpc":"2.0","id": 2, "result": {}},
            {"jsonrpc":"2.0","id": 2, "error": {"code": -32603, "message": "Internal error"}},
            {"jsonrpc":"2.0","id": 2, "error": {"code": -32602, "message": probe.EXPECTED_ERROR_MESSAGE}},
            {"jsonrpc":"2.0","id": 2, "error": {"code": -32603, "message": "API key rejected"}},
            {"jsonrpc":"2.0","id": 2, "error": {"code": -32603, "message": "Internal error", "data": {"details": "prefix: " + probe.EXPECTED_ERROR_MESSAGE}}},
            {"jsonrpc":"2.0","id": 2, "error": {"code": -32603, "message": "Internal error", "data": {"details": probe.EXPECTED_ERROR_MESSAGE, "other": True}}},
            {"jsonrpc":"2.0","id": 2, "error": {"code": -32600, "message": probe.EXPECTED_ERROR_MESSAGE, "data": {"details": probe.EXPECTED_ERROR_MESSAGE}}},
        ):
            with self.subTest(response=response), self.assertRaises(probe.ProbeFailure):
                probe.require_response(response, 2)

    def test_rejection_shape_is_static_and_redacted(self):
        shape = {}
        with self.assertRaisesRegex(probe.ProbeFailure, "acp_api_rejection_mismatch"):
            probe.require_response({"jsonrpc":"2.0", "id":2, "error":{
                "code":-32603, "message":"Internal error: secret sentinel",
                "data":{"cause": probe.EXPECTED_ERROR_MESSAGE + " secret sentinel"},
            }}, 2, shape)
        self.assertEqual(shape, {
            "codeClass":"internal_error", "messageContainsPolicySentence":False,
            "dataContainsPolicySentence":True, "hasData":True, "messageIsString":True,
        })
        self.assertNotIn("secret sentinel", repr(shape))

    def test_synthetic_peer_sequence_and_isolated_env(self):
        fixture = '''import json, os, pathlib, sys
assert os.environ["OPENAI_API_KEY"] == "sk-test-key"
assert "CODEX_API_KEY" not in os.environ and "DEFAULT_AUTH_REQUEST" not in os.environ
assert pathlib.Path(os.environ["CODEX_HOME"], "config.toml").read_text() == 'forced_login_method = "chatgpt"\\n'
first = json.loads(sys.stdin.readline())
assert first == {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":False,"writeTextFile":False},"terminal":False}}}
print(json.dumps({"jsonrpc":"2.0","id":1,"result":{"authMethods":[{"id":"api-key"}]}}), flush=True)
second = json.loads(sys.stdin.readline())
assert second == {"jsonrpc":"2.0","id":2,"method":"authenticate","params":{"methodId":"api-key"}}
print(json.dumps({"jsonrpc":"2.0","id":2,"error":{"code":-32603,"message":"Internal error","data":{"details":"API key login is disabled. Use ChatGPT login instead."}}}), flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "peer.py"
            entry.write_text(fixture)
            probe.run_adapter_probe(Path(sys.executable), entry, time.monotonic() + 5)

    def test_unexpected_success_fails_closed(self):
        fixture = '''import json, sys
sys.stdin.readline()
print(json.dumps({"jsonrpc":"2.0","id":1,"result":{"authMethods":[{"id":"api-key"}]}}), flush=True)
sys.stdin.readline()
print(json.dumps({"jsonrpc":"2.0","id":2,"result":{}}), flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "peer.py"
            entry.write_text(fixture)
            with self.assertRaisesRegex(probe.ProbeFailure, "acp_api_auth_not_rejected"):
                probe.run_adapter_probe(Path(sys.executable), entry, time.monotonic() + 5)

    def test_output_cap(self):
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "peer.py"
            entry.write_text('import sys\nsys.stdin.readline()\nsys.stderr.write("x"*70000)\nsys.stderr.flush()\n')
            with self.assertRaisesRegex(probe.ProbeFailure, "acp_output_limit"):
                probe.run_adapter_probe(Path(sys.executable), entry, time.monotonic() + 5)


if __name__ == "__main__":
    unittest.main()
