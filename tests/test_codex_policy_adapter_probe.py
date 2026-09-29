"""Synthetic ACP-only tests for the built subscription-policy probe."""

import json
from pathlib import Path
import sys
import tempfile
import time
import unittest

import codex_policy_adapter_probe as probe


def initialize_response(methods=None, caps=None):
    return {"jsonrpc": "2.0", "id": 1, "result": {
        "authMethods": methods if methods is not None else [{"id": "chat-gpt"}],
        "agentCapabilities": caps if caps is not None else {
            "sessionCapabilities": {"list": {}, "close": {}, "delete": {}},
            "mcpCapabilities": {"acp": False, "http": False, "sse": False},
        },
    }}


def denied_response(request_id):
    return {"jsonrpc": "2.0", "id": request_id, "error": {
        "code": -32603, "message": "Internal error",
        "data": {"details": probe.POLICY_ERROR},
    }}


class PolicyAdapterProbeTests(unittest.TestCase):
    def test_initialize_capabilities_and_auth_are_scoped(self):
        probe.require_response(initialize_response(), 1)
        for response in (
            initialize_response(methods=[{"id": "api-key"}]),
            initialize_response(caps={"providers": {}, "sessionCapabilities": {},
                                      "mcpCapabilities": {"http": False}}),
            initialize_response(caps={"sessionCapabilities": {"resume": {}},
                                      "mcpCapabilities": {"http": False}}),
            initialize_response(caps={"sessionCapabilities": {},
                                      "mcpCapabilities": {"http": True}}),
        ):
            with self.subTest(response=response), self.assertRaises(probe.ProbeFailure):
                probe.require_response(response, 1)

    def test_only_exact_policy_error_is_accepted(self):
        for request_id, _, _, _ in probe.REQUESTS:
            probe.require_response(denied_response(request_id), request_id)
        for response in (
            {"jsonrpc": "2.0", "id": 2, "result": {}},
            {"jsonrpc": "2.0", "id": 2, "error": {"code": -32602,
                                                         "message": probe.POLICY_ERROR}},
            {"jsonrpc": "2.0", "id": 2, "error": {"code": -32603,
                                                         "message": "Internal error",
                                                         "data": {"details": "other"}}},
        ):
            with self.subTest(response=response), self.assertRaises(probe.ProbeFailure):
                probe.require_response(response, 2)

    def test_fake_adapter_sees_only_safe_requests_and_clean_profile(self):
        source = '''import json, os, pathlib, sys
assert sys.argv[1:] == ["--require-chatgpt-subscription"]
home = pathlib.Path(os.environ["HOME"])
assert pathlib.Path(os.environ["CODEX_HOME"]).parent == home
assert "OPENAI_API_KEY" not in os.environ and "CODEX_API_KEY" not in os.environ
assert "DEFAULT_AUTH_REQUEST" not in os.environ and "CODEX_CONFIG" not in os.environ
for expected_id, method in [(1,"initialize"),(2,"authenticate"),(3,"providers/set"),
                            (4,"session/load"),(5,"session/resume"),(6,"session/fork")]:
    request = json.loads(sys.stdin.readline())
    assert request["id"] == expected_id and request["method"] == method
    assert method not in ("session/new", "session/prompt")
    if expected_id == 1:
        response = {"jsonrpc":"2.0","id":1,"result":{
            "authMethods":[{"id":"chat-gpt"}],
            "agentCapabilities":{"sessionCapabilities":{"list":{}},
                                 "mcpCapabilities":{"http":False}}}}
    else:
        response = {"jsonrpc":"2.0","id":expected_id,"error":{
            "code":-32603,"message":"Internal error",
            "data":{"details":"subscription_policy_denied"}}}
    print(json.dumps(response), flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            fake = Path(temporary) / "fake.py"
            fake.write_text(source)
            results = {"initializeConformant": False,
                       **{label: False for _, _, _, label in probe.REQUESTS}}
            probe.run_probe(Path(sys.executable), fake, time.monotonic() + 5, results)
        self.assertTrue(all(results.values()))

    def test_output_limit(self):
        with tempfile.TemporaryDirectory() as temporary:
            fake = Path(temporary) / "fake.py"
            fake.write_text('import sys\nsys.stdin.readline()\nsys.stderr.write("X"*70000)\nsys.stderr.flush()\n')
            with self.assertRaises(probe.ProbeFailure) as caught:
                probe.run_probe(Path(sys.executable), fake, time.monotonic() + 5, {})
        self.assertEqual(caught.exception.category, "acp_output_limit")


if __name__ == "__main__":
    unittest.main()
