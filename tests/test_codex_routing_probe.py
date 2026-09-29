"""Synthetic peers only; these tests never start Codex or a provider task."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

import codex_routing_probe as probe


class RoutingProbeTests(unittest.TestCase):
    def test_response_controls_fail_closed(self):
        probe._response({"jsonrpc": "2.0", "id": 1, "result": {
            "agentInfo": {"version": probe.ADAPTER_VERSION},
            "agentCapabilities": {"providers": {}},
        }}, 1)
        probe._response({"jsonrpc": "2.0", "id": 2, "error": {"code": -32603}}, 2)
        probe._response({"jsonrpc": "2.0", "id": 3, "result": {}}, 3)
        for response, expected in [
            ({"jsonrpc": "2.0", "id": 1, "result": {"agentInfo": None}}, 1),
            ({"jsonrpc": "2.0", "id": 2, "result": {}}, 2),
            ({"jsonrpc": "2.0", "id": 3, "error": {}}, 3),
            ({"jsonrpc": "2.0", "id": 4, "result": {}}, 4),
            ({"jsonrpc": "2.0", "id": 99, "result": {}}, 2),
        ]:
            with self.subTest(expected=expected, response=response), self.assertRaises(probe.ProbeFailure):
                probe._response(response, expected)

    def test_native_peer_rejects_thread_creation_and_records_only_flags(self):
        fixture = Path(probe.__file__).parent / "fixtures" / "codex_routing_native_peer.py"
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            state = home / "state.jsonl"
            env = {"CODEX_HOME": str(home), "CODEX_ROUTING_STATE": str(state)}
            lines = [
                {"id": 1, "method": "initialize", "params": {}},
                {"id": 2, "method": "account/read", "params": {"refreshToken": False}},
                {"id": 3, "method": "thread/start", "params": {"modelProvider": None, "config": {}}},
                {"id": 4, "method": "thread/start", "params": {"modelProvider": "custom-gateway", "config": {
                    "model_providers": {"custom-gateway": {"base_url": probe.GATEWAY}}}}},
            ]
            result = subprocess.run([sys.executable, str(fixture), "app-server"], env=env,
                                    input="".join(json.dumps(line) + "\n" for line in lines),
                                    text=True, capture_output=True, timeout=3, check=True)
            replies = [json.loads(line) for line in result.stdout.splitlines()]
            self.assertEqual([item["id"] for item in replies], [1, 2, 3, 4])
            self.assertTrue(all("error" in item for item in replies[2:]))
            probe._captures(state)
            self.assertNotIn(probe.GATEWAY, state.read_text())

    def test_native_capture_requires_two_exact_boolean_records(self):
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary) / "state"
            for payload in ("[]\n", '{"plainProvider":true,"customProvider":false,"customUrl":false,"noCustomConfig":true,"secret":"x"}\n'):
                state.write_text(payload)
                with self.assertRaises(probe.ProbeFailure):
                    probe._captures(state)

    def test_native_peer_rejects_turn_start_and_unknown_control(self):
        fixture = Path(probe.__file__).parent / "fixtures" / "codex_routing_native_peer.py"
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            env = {"CODEX_HOME": str(home), "CODEX_ROUTING_STATE": str(home / "state")}
            for method in ("turn/start", "account/login/start", "thread/resume"):
                result = subprocess.run([sys.executable, str(fixture), "app-server"], env=env,
                                        input=json.dumps({"id": 1, "method": method, "params": {}}) + "\n",
                                        text=True, capture_output=True, timeout=3)
                self.assertEqual(result.returncode, 6)
                self.assertEqual(result.stdout, "")
                self.assertFalse((home / "state").exists())

    def test_acp_sequence_uses_only_new_and_provider_set(self):
        fake_adapter = '''import json, os, pathlib, sys
assert "OPENAI_API_KEY" not in os.environ and "CODEX_API_KEY" not in os.environ
assert pathlib.Path(os.environ["CODEX_HOME"], "config.toml").read_text() == 'forced_login_method = "chatgpt"\\n'
state = pathlib.Path(os.environ["CODEX_ROUTING_STATE"])
for expected_id, method in [(1,"initialize"),(2,"session/new"),(3,"providers/set"),(4,"session/new")]:
    message=json.loads(sys.stdin.readline())
    assert message["id"]==expected_id and message["method"]==method
    if method=="session/new":
        assert message["params"]["mcpServers"]==[]
        capture = ({"plainProvider":True,"customProvider":False,"customUrl":False,"noCustomConfig":True}
                   if expected_id==2 else {"plainProvider":False,"customProvider":True,"customUrl":True,"noCustomConfig":False})
        with state.open("a") as output: output.write(json.dumps(capture)+"\\n")
        response={"error":{"code":-32603,"message":"synthetic stop"}}
    elif method=="providers/set":
        assert message["params"]=={"providerId":"openai","apiType":"openai","baseUrl":"https://routing-probe.invalid/v1","headers":{}}
        response={"result":{}}
    else:
        response={"result":{"agentInfo":{"version":"2.0.0"},"agentCapabilities":{"providers":{}}}}
    print(json.dumps({"jsonrpc":"2.0","id":expected_id,**response}),flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "adapter.py"
            entry.write_text(fake_adapter)
            probe.run_probe(Path(sys.executable), entry, time.monotonic() + 5)

    def test_adapter_output_cap(self):
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "adapter.py"
            entry.write_text('import sys\nsys.stderr.write("x"*70000)\nsys.stderr.flush()\nsys.stdin.readline()\n')
            with self.assertRaisesRegex(probe.ProbeFailure, "acp_output_limit"):
                probe.run_probe(Path(sys.executable), entry, time.monotonic() + 5)

    def test_timeout_reaps_adapter(self):
        with tempfile.TemporaryDirectory() as temporary:
            pid_file = Path(temporary) / "pid"
            entry = Path(temporary) / "adapter.py"
            entry.write_text("import os, pathlib, time\npathlib.Path(" + repr(str(pid_file)) + ").write_text(str(os.getpid()))\ntime.sleep(10)\n")
            with self.assertRaisesRegex(probe.ProbeFailure, "probe_timeout"):
                probe.run_probe(Path(sys.executable), entry, time.monotonic() + 1)
            pid = int(pid_file.read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)


if __name__ == "__main__":
    unittest.main()
