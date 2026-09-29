"""Synthetic native peer tests; no account, session, provider, or real Codex run."""

import json
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest

import codex_policy_native_probe as probe


class PolicyNativeProbeTests(unittest.TestCase):
    def test_exact_response_shape_and_static_flags(self):
        self.assertIsNone(probe._response({"id": 1, "result": {}}, 1))
        config = {"forced_login_method": "chatgpt", "model_provider": None}
        self.assertEqual(probe._response({"id": 2, "result": {"config": config}}, 2), config)
        self.assertTrue(all(probe._flags(config).values()))
        self.assertFalse(probe._flags({**config, "openai_base_url": "https://elsewhere.invalid"})["noUrlOverrides"])
        for response in ({"id": 2, "result": {}}, {"id": 2, "error": {}},
                         {"id": 3, "result": {"config": config}}):
            with self.subTest(response=response), self.assertRaises(probe.ProbeFailure):
                probe._response(response, 2)

    def test_synthetic_native_peer_receives_only_config_read(self):
        peer = '''import json, pathlib, sys
assert sys.argv[1:] == ["-c", "forced_login_method=chatgpt", "app-server"]
first = json.loads(sys.stdin.readline())
assert first["method"] == "initialize" and first["id"] == 1
print(json.dumps({"id": 1, "result": {}}), flush=True)
assert json.loads(sys.stdin.readline())["method"] == "initialized"
second = json.loads(sys.stdin.readline())
assert second["method"] == "config/read" and second["id"] == 2
assert second["params"]["includeLayers"] is False
assert pathlib.Path(second["params"]["cwd"]).is_dir()
print(json.dumps({"id": 2, "result": {"config": {
    "forced_login_method": "chatgpt", "model_provider": None}}}), flush=True)
'''
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "native.py"
            entry.write_text(peer)
            config = probe.read_native_config(Path(sys.executable), entry,
                                              Path(temporary) / "home", time.monotonic() + 5)
            self.assertTrue(all(probe._flags(config).values()))

    def test_policy_module_runs_with_type_stripping(self):
        node = shutil.which("node")
        if not node:
            self.skipTest("Node unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            module = home / "SubscriptionPolicy.ts"
            module.write_text("export function validSubscriptionConfig(value: unknown): boolean {\n"
                              "  return typeof value === 'object' && value !== null &&\n"
                              "    (value as {forced_login_method?: string}).forced_login_method === 'chatgpt';\n}\n")
            probe.check_policy_module(Path(node), module,
                                      {"forced_login_method": "chatgpt"}, home, time.monotonic() + 5)
            with self.assertRaises(probe.ProbeFailure):
                probe.check_policy_module(Path(node), module,
                                          {"forced_login_method": "api"}, home, time.monotonic() + 5)

    def test_native_output_cap(self):
        with tempfile.TemporaryDirectory() as temporary:
            entry = Path(temporary) / "native.py"
            entry.write_text('import sys\nsys.stdin.readline()\nsys.stderr.write("x" * 70000)\nsys.stderr.flush()\n')
            with self.assertRaisesRegex(probe.ProbeFailure, "native_output_limit"):
                probe.read_native_config(Path(sys.executable), entry,
                                         Path(temporary) / "home", time.monotonic() + 5)


if __name__ == "__main__":
    unittest.main()
