"""Synthetic wire and process tests for the compiled Claude policy probe."""

from pathlib import Path
import sys
import tempfile
import time
import unittest

import claude_policy_adapter_probe as probe


def initialize_response(methods=None, caps=None):
    return {"jsonrpc": "2.0", "id": 1, "result": {
        "authMethods": methods if methods is not None else [{
            "id": "claude-ai-login", "type": "terminal",
            "args": ["--require-claude-subscription", "--cli", "auth", "login", "--claudeai"],
            "_meta": {"terminal-auth": {"args": ["adapter", "--require-claude-subscription",
                                               "--cli", "auth", "login", "--claudeai"]}},
        }],
        "agentCapabilities": caps if caps is not None else {
            "sessionCapabilities": {"list": {}, "close": {}, "delete": {}},
            "mcpCapabilities": {"http": False, "sse": False},
        },
        "_meta": {},
    }}


def denied_response(request_id):
    return {"jsonrpc": "2.0", "id": request_id, "error": {
        "code": -32000, "message": probe.POLICY_MESSAGE,
        "data": {"reason": probe.POLICY_REASON},
    }}


class ClaudePolicyAdapterProbeTests(unittest.TestCase):
    def test_initialize_advertises_only_local_subscription_login(self):
        probe.require_response(initialize_response(), 1)
        original = initialize_response()
        for methods in (
            [], [{"id": "console-login"}], [{"id": "gateway"}],
            [*original["result"]["authMethods"], {"id": "claude-login"}],
            [{**original["result"]["authMethods"][0], "args": ["--cli", "auth", "login", "--console"]}],
            [{**original["result"]["authMethods"][0], "_meta": {}}],
        ):
            with self.subTest(methods=methods), self.assertRaises(probe.ProbeFailure):
                probe.require_response(initialize_response(methods=methods), 1)

    def test_initialize_omits_denied_capabilities(self):
        base = initialize_response()["result"]["agentCapabilities"]
        for changed in (
            {"providers": {}}, {"loadSession": True},
            {"sessionCapabilities": {"resume": {}}},
            {"sessionCapabilities": {"fork": {}}},
            {"mcpCapabilities": {"http": True, "sse": False}},
            {"mcpCapabilities": {"http": False, "sse": True}},
        ):
            with self.subTest(changed=changed), self.assertRaises(probe.ProbeFailure):
                probe.require_response(initialize_response(caps={**base, **changed}), 1)
        response = initialize_response()
        response["result"]["_meta"] = {"steering": {"supported": True}}
        with self.assertRaises(probe.ProbeFailure):
            probe.require_response(response, 1)

    def test_only_exact_policy_error_is_accepted(self):
        for request_id, _, _, _ in probe.REQUESTS:
            probe.require_response(denied_response(request_id), request_id)
        for response in (
            {"jsonrpc": "2.0", "id": 2, "result": {}},
            {**denied_response(2), "id": True},
            {**denied_response(2), "error": {"code": -32603,
                                                "message": probe.POLICY_MESSAGE,
                                                "data": {"reason": probe.POLICY_REASON}}},
            {**denied_response(2), "error": {"code": -32000,
                                                "message": probe.POLICY_MESSAGE,
                                                "data": {"reason": "other"}}},
        ):
            with self.subTest(response=response), self.assertRaises(probe.ProbeFailure):
                probe.require_response(response, 2)

    def test_fake_adapter_sees_only_safe_requests_and_clean_profile(self):
        source = '''import json, os, pathlib, sys
assert sys.argv[1:] == ["--require-claude-subscription"]
home = pathlib.Path(os.environ["HOME"])
assert pathlib.Path(os.environ["CLAUDE_CONFIG_DIR"]).parent == home
assert "ANTHROPIC_API_KEY" not in os.environ and "CLAUDE_CODE_OAUTH_TOKEN" not in os.environ
assert "HTTPS_PROXY" not in os.environ and "CLAUDE_CODE_EXECUTABLE" not in os.environ
expected = [(1,"initialize"),(2,"authenticate"),(3,"authenticate"),
            (4,"providers/set"),(5,"providers/disable"),(6,"session/load"),
            (7,"session/resume"),(8,"session/fork"),(9,"_session/steering")]
for request_id, method in expected:
    request = json.loads(sys.stdin.readline())
    assert request["id"] == request_id and request["method"] == method
    assert method not in ("session/new", "session/prompt")
    if request_id == 1:
        assert request["params"]["clientCapabilities"]["auth"]["terminal"] is True
        response = {"jsonrpc":"2.0","id":1,"result":{
            "authMethods":[{"id":"claude-ai-login","type":"terminal",
                "args":["--require-claude-subscription","--cli","auth","login","--claudeai"],
                "_meta":{"terminal-auth":{"args":["adapter","--require-claude-subscription",
                    "--cli","auth","login","--claudeai"]}}}],
            "agentCapabilities":{"sessionCapabilities":{"list":{}},
                                 "mcpCapabilities":{"http":False,"sse":False}},
            "_meta":{}}}
    else:
        response = {"jsonrpc":"2.0","id":request_id,"error":{
            "code":-32000,"message":"Claude subscription route is unavailable.",
            "data":{"reason":"claude_subscription_route_unavailable"}}}
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
