#!/usr/bin/env python3
"""Mock-only runner safety/lifecycle checks; no Docker or network required."""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
loader = importlib.machinery.SourceFileLoader("relay_runner", str(ROOT / "scripts/relay-conformance-runner"))
spec = importlib.util.spec_from_loader(loader.name, loader)
runner = importlib.util.module_from_spec(spec)
loader.exec_module(runner)
PASSED = b"test real_relay_tests::real_relay_messaging_conformance ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 89 filtered out; finished in 0.1s\n"


ACP_PASSED = b"test acp_relay_tests::acp_relay_synthetic_routing ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 89 filtered out; finished in 0.1s\n"


class Response:
    status = 200
    def __init__(self, content):
        self.content = content
    def __enter__(self):
        return self
    def __exit__(self, *_):
        return False
    def read(self, _limit):
        return self.content


class Opener:
    def open(self, request, timeout):
        url = request if isinstance(request, str) else request.full_url
        assert url.startswith("http://127.0.0.1:")
        return Response(json.dumps({"self": "a" * 64}).encode() if url.endswith("/info") else b"{}")


class RunnerTests(unittest.TestCase):
    def test_only_loopback_publication_clean_environment_and_fixed_command(self):
        project = "obuzz-test-" + "a" * 24
        plan = runner.compose_plan(project, Path("/tmp/binary with spaces"), [43210, 43211], "synthetic-password", "b" * 64)
        self.assertTrue(plan["networks"]["fixture"]["internal"])
        for name, service in plan["services"].items():
            self.assertNotIn("container_name", service)
            self.assertNotIn("external", service)
            for port in service.get("ports", []):
                self.assertTrue(port.startswith("127.0.0.1:"))
            if name in ("minio", "redis", "minio-init"):
                self.assertNotIn("ports", service)
        relay = plan["services"]["relay"]
        self.assertEqual(relay["ports"], ["127.0.0.1:43211:3000"])
        self.assertTrue(relay["volumes"][0]["read_only"])
        self.assertTrue(relay["read_only"])
        with tempfile.TemporaryDirectory() as temp:
            environment = runner.clean_environment(temp, {"PATH": "/trusted/tools", "CARGO_HOME": "/trusted/cargo", "GH_TOKEN": "must-not-pass", "DATABASE_URL": "production", "DOCKER_HOST": "production"})
            self.assertNotIn("GH_TOKEN", environment)
            self.assertNotIn("DATABASE_URL", environment)
            self.assertNotIn("DOCKER_HOST", environment)
            self.assertEqual(environment["CARGO_HOME"], "/trusted/cargo")
            self.assertEqual((Path(environment["HOME"]).stat().st_mode & 0o777), 0o700)
        command = runner.helper_command("/source with spaces", "/target with spaces")
        self.assertIn("real_relay_", command)
        self.assertEqual(command[-3:], ["--", "--ignored", "--test-threads=1"])

    def test_messaging_only_omits_object_storage_and_reports_scope(self):
        plan = runner.compose_plan("obuzz-test-" + "a" * 24, Path("/tmp/binary"), [43210, 43211], "fixture", "b" * 64, True)
        self.assertEqual(set(plan["services"]), {"postgres", "redis", "relay"})
        self.assertEqual(set(plan["volumes"]), {"postgres-data"})
        environment = plan["services"]["relay"]["environment"]
        self.assertEqual(environment["BUZZ_GIT_CONFORMANCE_PROBE"], "false")
        self.assertEqual(environment["BUZZ_S3_ENDPOINT"], "http://127.0.0.1:9")
        status, summary = self.run_mock(messaging_only=True)
        self.assertEqual(status, 0)
        self.assertEqual(summary["validationScope"], "messaging-only")
        self.assertFalse(summary["objectStoreProbe"])
        self.assertEqual(set(summary["images"]), {"postgres", "redis", "relay"})

    def test_zero_tests_or_wrong_fixture_never_passes(self):
        self.assertEqual(runner.successful_tests(PASSED)["passed"], 1)
        for output in (b"test result: ok. 0 passed; 0 failed; 0 ignored;", PASSED.replace(b" ... ok", b" ... ignored"), PASSED.replace(b"1 passed", b"0 passed")):
            with self.assertRaises(ValueError):
                runner.successful_tests(output)

    def run_mock(self, fixture_output=PASSED, fail_build=False, collision=False, messaging_only=False, acp=False, acp_output=ACP_PASSED):
        with tempfile.TemporaryDirectory(prefix="runner mock spaces ") as temp:
            base = Path(temp)
            source = base / "buzz"
            helper = base / "helper-repo"
            source.mkdir()
            helper.mkdir()
            (source / "scripts").mkdir()
            (source / "scripts/reconcile-schema-after-pgschema.sql").write_text("-- fixture SQL\n")
            binary = base / "relay binary"
            binary.write_bytes(b"never executed fixture")
            binary.chmod(0o700)
            acp_bins = base / "acp tools"
            acp_bins.mkdir()
            node = base / "node fixture"
            for executable in (node, *(acp_bins / name for name in ("buzz-acp", "buzz", "git-sign-nostr", "git-credential-nostr"))):
                executable.write_bytes(b"never executed fixture")
                executable.chmod(0o700)
            calls = []
            def command(args, env, cwd, log, **kwargs):
                calls.append(list(args))
                self.assertNotIn("GH_TOKEN", env)
                if args[0] == "git":
                    return (runner.REVISION + "\n").encode() if "rev-parse" in args else b""
                if args[:3] == ["docker", "volume", "ls"] and collision:
                    return b"preexisting-owned-label-volume"
                if args[:2] == ["docker", "build"] and fail_build:
                    raise RuntimeError("mocked failure")
                if args[:2] == ["docker", "inspect"]:
                    return ("sha256:" + "a" * 64).encode()
                if "ps" in args and "-q" in args:
                    return b"fixture-container-id"
                if args[0] == "cargo":
                    if "acp_relay_" in args:
                        self.assertEqual(env["OMARCHY_BUZZ_TEST_ACP_BUZZ_SOURCE"], str(source))
                        self.assertEqual(env["OMARCHY_BUZZ_TEST_ACP_BIN_DIR"], str(acp_bins))
                        self.assertEqual(env["OMARCHY_BUZZ_TEST_ACP_NODE"], str(node))
                        self.assertEqual(env["OMARCHY_BUZZ_TEST_RELAY_URL"], "ws://127.0.0.1:43211")
                        self.assertEqual(kwargs["timeout"], 240)
                        self.assertTrue(any("real_relay_" in previous for previous in calls[:-1]))
                        return acp_output
                    return fixture_output
                return b""
            process = SimpleNamespace(poll=lambda: None)
            arguments = SimpleNamespace(buzz_source=source, relay_binary=binary, helper_source=helper,
                                        helper_target=base / "target", output=base / "output", messaging_only=messaging_only,
                                        acp_bin_dir=acp_bins if acp else None, acp_node=node if acp else None)
            with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "GH_TOKEN": "must-not-pass"}), \
                    patch.object(runner, "command", command), patch.object(runner.subprocess, "Popen", return_value=process), \
                    patch.object(runner, "reserved_ports", return_value=([SimpleNamespace(close=lambda: None)], [43210, 43211])), \
                    patch.object(runner, "stop_group"), patch.object(runner.urllib.request, "build_opener", return_value=Opener()), \
                    contextlib.redirect_stdout(io.StringIO()):
                status = runner.run(arguments)
            summary = json.loads((arguments.output / "summary.json").read_text())
            self.assertTrue(summary["cleanupComplete"])
            if collision:
                self.assertFalse(any("down" in call or "up" in call or "build" in call for call in calls))
            else:
                cleanup = next(call for call in calls if "down" in call)
                self.assertIn("--volumes", cleanup)
                self.assertTrue(cleanup[cleanup.index("--project-name") + 1].startswith("obuzz-test-"))
                self.assertNotIn("prune", " ".join(cleanup))
            self.assertNotIn("must-not-pass", json.dumps(summary))
            self.assertNotIn("PRIVATE_KEY", json.dumps(summary))
            if summary["messaging"]["state"] != "passed":
                self.assertFalse(any("acp_relay_" in call for call in calls))
            self.assertEqual((arguments.output / "private-runner.log").stat().st_mode & 0o777, 0o600)
            return status, summary

    def test_optional_acp_stage_requires_exact_test_and_prior_messaging(self):
        self.assertEqual(runner.successful_tests(ACP_PASSED, acp=True)["requiredTest"], "acp_relay_tests::acp_relay_synthetic_routing")
        for output in (PASSED, ACP_PASSED.replace(b"1 passed", b"0 passed"), ACP_PASSED.replace(b" ... ok", b" ... ignored")):
            with self.assertRaises(ValueError):
                runner.successful_tests(output, acp=True)
        status, report = self.run_mock(acp=True)
        self.assertEqual(status, 0)
        self.assertEqual(report["messaging"]["state"], "passed")
        self.assertEqual(report["acp"]["state"], "passed")
        self.assertTrue(report["acp"]["syntheticOnly"])
        status, report = self.run_mock(acp=True, acp_output=b"test result: ok. 0 passed; 0 failed; 0 ignored;")
        self.assertEqual(status, 1)
        self.assertEqual(report["messaging"]["state"], "passed")
        self.assertEqual(report["acp"]["state"], "failed")
        self.assertEqual(report["stage"], "synthetic_acp_conformance")
        status, report = self.run_mock(acp=True, fixture_output=b"test result: ok. 0 passed; 0 failed; 0 ignored;")
        self.assertEqual(status, 1)
        self.assertEqual(report["messaging"]["state"], "failed")
        self.assertEqual(report["acp"]["state"], "not_run")

    def test_acp_options_must_be_paired_before_any_operation(self):
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}), patch.object(runner, "command") as command:
            for options in ({"acp_bin_dir": Path("/fixture/tools")}, {"acp_node": Path("/fixture/node")}):
                with self.assertRaises(ValueError):
                    runner.run(SimpleNamespace(**options))
            command.assert_not_called()
        with tempfile.TemporaryDirectory() as temporary:
            binaries = Path(temporary)
            node = binaries / "node"
            node.write_bytes(b"not executed")
            node.chmod(0o700)
            with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}), patch.object(runner, "command") as command:
                with self.assertRaises(ValueError):
                    runner.run(SimpleNamespace(acp_bin_dir=binaries, acp_node=node))
                command.assert_not_called()

    def test_full_mock_pass_and_cleanup(self):
        status, summary = self.run_mock()
        self.assertEqual(status, 0)
        self.assertEqual(summary["tests"]["passed"], 1)
        self.assertEqual(summary["messaging"]["state"], "passed")
        self.assertEqual(summary["acp"]["state"], "not_requested")
        self.assertEqual(set(summary["images"]), {"postgres", "redis", "minio", "relay"})

    def test_build_failure_and_zero_fixture_still_cleanup(self):
        for options in ({"fail_build": True}, {"fixture_output": b"test result: ok. 0 passed; 0 failed; 0 ignored;"}):
            status, summary = self.run_mock(**options)
            self.assertEqual(status, 1)
            self.assertEqual(summary["state"], "failed")

    def test_existing_volume_is_never_reused_or_removed(self):
        status, summary = self.run_mock(collision=True)
        self.assertEqual(status, 1)
        self.assertEqual(summary["stage"], "preflight")

    def test_failure_summary_excludes_arguments_and_raw_diagnostics(self):
        error = runner.FixtureCommandError(["docker", "compose", "--file", "/private/secret.json", "up"], 1,
                                          b"manifest unknown: secret-password signed-event-content")
        self.assertEqual(error.safe, {"tool": "docker", "action": "up", "exitCode": 1, "category": "image_not_found"})
        self.assertNotIn("secret", json.dumps(error.safe))
        timeout = runner.FixtureCommandError(["/private/path/pgschema", "private-argument"], None, timed_out=True)
        self.assertEqual(timeout.safe["category"], "timeout")
        self.assertEqual(timeout.safe["tool"], "pgschema")

    def test_non_disposable_preflight_does_not_touch_docker(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(runner, "command") as command:
            with self.assertRaises(ValueError):
                runner.run(SimpleNamespace())
            command.assert_not_called()


if __name__ == "__main__":
    unittest.main()
