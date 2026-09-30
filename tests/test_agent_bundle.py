"""Synthetic checks for harness bundle assembly/check and provider sign-in helpers."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"


def load(name):
    loader = importlib.machinery.SourceFileLoader(name.replace("-", "_"), str(SCRIPTS / name))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


bundle_tool = load("agent-bundle")
login_tool = load("agent-login")
ELF = b"\x7fELF\x02\x01\x01\x00" + b"\x00" * 8 + b"\x02\x00\xb7\x00" + b"synthetic"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(script, *args, env=None):
    return subprocess.run([sys.executable, str(SCRIPTS / script), *args], capture_output=True,
                          text=True, timeout=60, env=env, stdin=subprocess.DEVNULL)


def synthetic_ci_artifact(root):
    """CI artifact layout (stock-agent/bin + build.json) with synthetic ARM64 ELF files."""
    artifact = root / "stock-agent"
    (artifact / "bin").mkdir(parents=True)
    files = {}
    for name in ("buzz-acp", "buzz", "buzz-admin", "node"):
        data = ELF + name.encode()
        (artifact / "bin" / name).write_bytes(data)
        (artifact / "bin" / name).chmod(0o755)
        files["bin/" + name] = sha(data)
    (artifact / "LICENSE").write_text("synthetic license\n")
    (artifact / "build.json").write_text(json.dumps({
        "sourceRevision": bundle_tool.BUZZ["revision"], "patchesApplied": [], "runId": "1",
        "workflowRevision": "0" * 40, "files": files}))
    return artifact / "bin"


def synthetic_adapter(root, harness):
    spec = bundle_tool.ADAPTERS[harness]
    adapter = root / ("adapter-" + harness)
    root.mkdir(parents=True, exist_ok=True)
    (adapter / "node_modules").mkdir(parents=True)
    (adapter / "package.json").write_text("{}")
    (adapter / "package-lock.json").write_text("{}")
    record = {}
    for package, (version, integrity) in spec["packages"].items():
        (adapter / package).mkdir(parents=True)
        (adapter / package / "package.json").write_text(json.dumps({"version": version}))
        record[package] = {"version": version, "integrity": integrity}
    (adapter / "node_modules/.package-lock.json").write_text(json.dumps({"packages": record}))
    entry = adapter / spec["entry"]
    entry.parent.mkdir(parents=True, exist_ok=True)
    entry.write_text("// synthetic entry\n")
    for name in spec["native"]:
        (adapter / name).parent.mkdir(parents=True, exist_ok=True)
        (adapter / name).write_bytes(b"native")
        (adapter / name).chmod(0o755)
    (adapter / "node_modules/.bin").mkdir()
    (adapter / "node_modules/.bin/tool").symlink_to("../x")
    unused = adapter / "node_modules/@anthropic-ai/claude-agent-sdk-linux-arm64"
    unused.mkdir(parents=True)
    (unused / "claude").write_bytes(b"unused native copy")
    return adapter


class Arguments:
    def __init__(self, **values):
        self.__dict__.update(values)


class BundleAssembly(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.saved = (bundle_tool.node_binary, dict(bundle_tool.CLAUDE_CLI),
                      {k: dict(v["native"]) for k, v in bundle_tool.ADAPTERS.items()})
        # The real pins name the verified binaries; synthetic stand-ins get synthetic pins.
        bundle_tool.node_binary = lambda path: path
        self.cli = self.root / "claude-cli"
        self.cli.write_bytes(ELF + b"claude")
        bundle_tool.CLAUDE_CLI["sha256"] = sha(ELF + b"claude")
        bundle_tool.ADAPTERS["codex"]["native"] = {bundle_tool.CODEX_NATIVE: sha(b"native")}

    def tearDown(self):
        bundle_tool.node_binary, cli, native = self.saved
        bundle_tool.CLAUDE_CLI.update(cli)
        for harness, pins in native.items():
            bundle_tool.ADAPTERS[harness]["native"] = pins
        self.temporary.cleanup()

    def assemble(self, harness, **overrides):
        self.count = getattr(self, "count", 0) + 1
        values = dict(harness=harness, output=self.root / ("agent-" + harness), node=None,
                      npm_ci=False, npm_cli=None, claude_cli=self.cli)
        values.update(overrides)
        if "buzz_bin" not in values:
            values["buzz_bin"] = synthetic_ci_artifact(self.root / f"ci-{self.count}")
        if "adapter" not in values:
            values["adapter"] = synthetic_adapter(self.root / f"adapter-{self.count}", harness)
        return bundle_tool.assemble(Arguments(**values))

    def test_claude_code_bundle_layout_and_check(self):
        bundle = self.assemble("claude-code")
        record = json.loads((bundle / "bundle.json").read_text())
        self.assertEqual(record["harness"], "claude-code")
        self.assertEqual(record["versions"]["@agentclientprotocol/claude-agent-acp"], "0.82.0")
        self.assertEqual(record["versions"]["claude"], "2.1.280")
        self.assertEqual(record["buzz"]["source"], "ci-build-json")
        self.assertEqual((bundle / "bin/claude-agent-acp").read_bytes(), (SCRIPTS / "room-claude-acp").read_bytes())
        self.assertEqual((bundle / "bin/claude").read_bytes(), (SCRIPTS / "room-claude").read_bytes())
        self.assertEqual(json.loads((bundle / "claude/subscription-settings.json").read_text()),
                         {"forceLoginMethod": "claudeai"})
        self.assertFalse((bundle / "adapter/node_modules/.bin").exists())
        self.assertFalse((bundle / "adapter/node_modules/@anthropic-ai/claude-agent-sdk-linux-arm64").exists())
        self.assertEqual(oct((bundle / "bin/buzz-acp").stat().st_mode & 0o777), "0o755")
        for name in ("room-agent", "room-sandbox", "agent-login", "agent-bundle"):
            self.assertTrue((bundle / "launcher" / name).is_file())
        self.assertEqual(bundle_tool.check(bundle, "claude-code")["harness"], "claude-code")
        with self.assertRaisesRegex(bundle_tool.Refused, "bundle_harness_mismatch"):
            bundle_tool.check(bundle, "codex")
        self.assertEqual(sorted(p.name for p in self.root.iterdir() if p.name.startswith(".")), [])

    def test_codex_bundle_layout(self):
        bundle = self.assemble("codex")
        self.assertEqual((bundle / "bin/codex-acp").read_bytes(), (SCRIPTS / "room-codex-acp").read_bytes())
        self.assertTrue((bundle / "adapter" / bundle_tool.CODEX_NATIVE).is_file())
        self.assertFalse((bundle / "claude").exists())
        bundle_tool.check(bundle, "codex")

    def test_assembly_refusals(self):
        existing = self.root / "agent-codex"
        existing.mkdir()
        with self.assertRaisesRegex(bundle_tool.Refused, "bundle_output_exists"):
            self.assemble("codex")
        with self.assertRaisesRegex(bundle_tool.Refused, "absolute_output_required"):
            self.assemble("codex", output=Path("relative"))
        with self.assertRaisesRegex(bundle_tool.Refused, "adapter_source_required"):
            self.assemble("claude-code", adapter=None)
        adapter = synthetic_adapter(self.root / "bad", "claude-code")
        record = json.loads((adapter / "node_modules/.package-lock.json").read_text())
        record["packages"]["node_modules/@anthropic-ai/claude-agent-sdk"]["integrity"] = "sha512-other"
        (adapter / "node_modules/.package-lock.json").write_text(json.dumps(record))
        with self.assertRaisesRegex(bundle_tool.Refused, "adapter_integrity_mismatch"):
            self.assemble("claude-code", adapter=adapter)
        adapter = synthetic_adapter(self.root / "linked", "claude-code")
        (adapter / "node_modules/extra-link").symlink_to("/etc/hostname")
        with self.assertRaisesRegex(bundle_tool.Refused, "adapter_tree_unsafe"):
            self.assemble("claude-code", adapter=adapter)
        self.cli.write_bytes(ELF + b"other claude")
        with self.assertRaisesRegex(bundle_tool.Refused, "claude_cli_unpinned"):
            self.assemble("claude-code", output=self.root / "agent-claude-2")
        leftovers = [p.name for p in self.root.iterdir() if p.name.startswith(".agent-")]
        self.assertEqual(leftovers, [])

    def test_unverified_buzz_binaries_are_refused(self):
        bin_dir = self.root / "loose/bin"
        bin_dir.mkdir(parents=True)
        for name in ("buzz-acp", "buzz", "buzz-admin"):
            (bin_dir / name).write_bytes(ELF + b"unpinned")
        with self.assertRaisesRegex(bundle_tool.Refused, "buzz_binary_hash_mismatch"):
            self.assemble("codex", buzz_bin=bin_dir)
        artifact = synthetic_ci_artifact(self.root / "wrong-revision")
        build = artifact.parent / "build.json"
        report = json.loads(build.read_text())
        report["sourceRevision"] = "f" * 40
        build.write_text(json.dumps(report))
        with self.assertRaisesRegex(bundle_tool.Refused, "buzz_revision_mismatch"):
            self.assemble("codex", buzz_bin=artifact)


class BundleCheck(unittest.TestCase):
    """--check on a synthetic tree written directly, through the command line."""

    def make(self, root, harness="codex"):
        bundle = root / ("agent-" + harness)
        for relative in bundle_tool.entrypoints(harness).values():
            (bundle / relative).parent.mkdir(parents=True, exist_ok=True)
            (bundle / relative).write_bytes(b"content of " + relative.encode())
            (bundle / relative).chmod(0o755)
        files = {rel: {"sha256": bundle_tool.digest(item), "bytes": item.stat().st_size,
                       "mode": format(item.stat().st_mode & 0o777, "04o")}
                 for rel, item in bundle_tool.walk_bundle(bundle)}
        (bundle / "bundle.json").write_text(json.dumps({
            "schemaVersion": 1, "harness": harness, "entrypoints": bundle_tool.entrypoints(harness),
            "files": files}))
        return bundle

    def check(self, bundle, harness="codex"):
        return run("agent-bundle", harness, "--check", "--output", str(bundle))

    def test_valid_missing_and_changed(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = self.make(Path(temporary))
            result = self.check(bundle)
            self.assertEqual((result.returncode, result.stdout.strip()), (0, "ready"), result.stderr)
            (bundle / "launcher/__pycache__").mkdir()
            (bundle / "launcher/__pycache__/x.pyc").write_bytes(b"cache")
            self.assertEqual(self.check(bundle).returncode, 0)

            def expect(category):
                result = self.check(bundle)
                self.assertEqual((result.returncode, result.stdout.strip()), (1, "missing"))
                self.assertEqual(json.loads(result.stderr), {"error": category})

            node = bundle / "bin/node"
            original = node.read_bytes()
            node.write_bytes(original[:-1] + b"X")
            expect("bundle_hash_mismatch")
            node.write_bytes(original)
            node.chmod(0o777)
            expect("bundle_hash_mismatch")
            node.chmod(0o755)
            (bundle / "bin/extra").write_text("unrecorded")
            expect("bundle_unexpected_file")
            (bundle / "bin/extra").unlink()
            (bundle / "bin/link").symlink_to("node")
            expect("bundle_link_refused")
            (bundle / "bin/link").unlink()
            node.unlink()
            expect("bundle_file_missing")
            (bundle / "bundle.json").write_text("{")
            expect("bundle_manifest_invalid")
            result = self.check(Path(temporary) / "absent")
            self.assertEqual(result.stdout.strip(), "missing")
            self.assertEqual(json.loads(result.stderr), {"error": "bundle_missing"})

    def test_harness_mismatch_and_unknown_harness(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = self.make(Path(temporary), "claude-code")
            self.assertEqual(self.check(bundle, "claude-code").returncode, 0)
            result = self.check(bundle, "codex")
            self.assertEqual(json.loads(result.stderr), {"error": "bundle_harness_mismatch"})
            result = run("agent-bundle", "goose", "--check")
            self.assertEqual(result.returncode, 2)
            self.assertEqual(json.loads(result.stderr), {"error": "harness_unknown"})


class AgentLogin(unittest.TestCase):
    def status(self, profile, harness):
        result = run("agent-login", "--status", harness, "--profile", str(profile))
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def test_unknown_harness_is_refused(self):
        for args in (("goose",), ("--status", "goose"), ("claude",), ("--dry-run", "../codex")):
            result = run("agent-login", *args)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(json.loads(result.stderr), {"error": "harness_unknown"})
            self.assertEqual(result.stdout, "")

    def test_status_mapping_reads_metadata_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile = root / "claude-code"
            self.assertEqual(self.status(profile, "claude-code"), "signed-out")
            (profile / "provider").mkdir(parents=True, mode=0o700)
            self.assertEqual(self.status(profile, "claude-code"), "signed-out")
            credential = profile / "provider/.credentials.json"
            credential.write_text("")
            self.assertEqual(self.status(profile, "claude-code"), "signed-out")
            credential.write_text("synthetic, never read")
            credential.chmod(0o000)  # unreadable: the check must not need to open it
            self.assertEqual(self.status(profile, "claude-code"), "signed-in")
            self.assertEqual(self.status(profile, "codex"), "signed-out")
            credential.chmod(0o600)
            (profile / "provider/auth.json").write_text("synthetic")
            self.assertEqual(self.status(profile, "codex"), "signed-in")
            credential.unlink()
            credential.symlink_to(profile / "provider/auth.json")
            self.assertEqual(self.status(profile, "claude-code"), "unknown")
            credential.unlink()
            credential.mkdir()
            self.assertEqual(self.status(profile, "claude-code"), "unknown")
            linked = root / "linked"
            linked.symlink_to(profile)
            self.assertEqual(self.status(linked, "codex"), "unknown")
            (root / "file-profile").write_text("")
            self.assertEqual(self.status(root / "file-profile", "codex"), "unknown")

    def fake_bundle(self, root, harness):
        bundle = root / ("agent-" + harness)
        native = bundle / login_tool.NATIVE[harness]
        native.parent.mkdir(parents=True)
        native.write_text("#!/bin/sh\n")
        native.chmod(0o755)
        if harness == "claude-code":
            (bundle / "claude/subscription-settings.json").write_text('{"forceLoginMethod": "claudeai"}')
        return bundle

    def test_login_commands_and_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            codex = self.fake_bundle(root, "codex")
            claude = self.fake_bundle(root, "claude-code")
            self.assertEqual(login_tool.login_command(codex, "codex"),
                             [str(codex / login_tool.NATIVE["codex"]), "-c", 'forced_login_method="chatgpt"', "login"])
            self.assertEqual(login_tool.login_command(claude, "claude-code"),
                             [str(claude / "claude/claude"), "--settings",
                              str(claude / "claude/subscription-settings.json"), "auth", "login", "--claudeai"])
            with self.assertRaisesRegex(login_tool.Refused, "harness_missing"):
                login_tool.login_command(root / "absent", "codex")
            ambient = {"ANTHROPIC_API_KEY": "x", "OPENAI_API_KEY": "x", "CLAUDE_CODE_OAUTH_TOKEN": "x",
                       "NODE_OPTIONS": "x", "HTTPS_PROXY": "x", "BUZZ_PRIVATE_KEY": "x",
                       "WAYLAND_DISPLAY": "wayland-1", "TERM": "xterm"}
            profile = root / "profile"
            env = login_tool.login_environment(profile, "claude-code", ambient)
            self.assertEqual(env["CLAUDE_CONFIG_DIR"], str(profile / "provider"))
            self.assertEqual(env["HOME"], str(profile / "home"))
            self.assertEqual(env["WAYLAND_DISPLAY"], "wayland-1")
            for key in ("ANTHROPIC_API_KEY", "OPENAI_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN", "NODE_OPTIONS",
                        "HTTPS_PROXY", "BUZZ_PRIVATE_KEY", "CODEX_HOME"):
                self.assertNotIn(key, env)
            self.assertEqual(login_tool.login_environment(profile, "codex", {})["CODEX_HOME"], str(profile / "provider"))

    def test_terminal_launch_dry_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = self.fake_bundle(root, "claude-code")
            tools = root / "tools"
            tools.mkdir()
            home = root / "home"
            home.mkdir()
            profile = root / "state/claude-code"
            env = {"HOME": str(home), "PATH": str(tools)}
            args = ("claude-code", "--dry-run", "--bundle", str(bundle), "--profile", str(profile))
            result = run("agent-login", *args, env=env)
            self.assertEqual(json.loads(result.stderr), {"error": "terminal_unavailable"})
            (tools / "xdg-terminal-exec").write_text("#!/bin/sh\n")
            (tools / "xdg-terminal-exec").chmod(0o755)
            result = run("agent-login", *args, env=env)
            argv = json.loads(result.stdout)
            self.assertEqual(argv[:3], [str(tools / "xdg-terminal-exec"), "--title=Buzz agent sign-in", "-e"])
            inner = argv[3:]
            self.assertEqual(inner[2:], ["--in-terminal", "claude-code", "--bundle", str(bundle),
                                         "--profile", str(profile)])
            for name in login_tool.PROFILE_DIRS:
                self.assertEqual((profile / name).stat().st_mode & 0o777, 0o700)
            floating = tools / login_tool.FLOATING
            floating.write_text("#!/bin/sh\n")
            floating.chmod(0o755)
            argv = json.loads(run("agent-login", *args, env=env).stdout)
            self.assertEqual(argv[0], str(floating))
            self.assertEqual(shlex.split(argv[1]), inner)
            result = run("agent-login", "codex", "--dry-run", "--bundle", str(root / "absent"),
                         "--profile", str(root / "state/codex"), env=env)
            self.assertEqual((result.returncode, json.loads(result.stderr)), (1, {"error": "harness_missing"}))
            self.assertFalse((root / "state/codex").exists())
            result = run("agent-login", "claude-code", "--in-terminal", "--bundle", str(bundle),
                         "--profile", str(profile), env=env)
            self.assertEqual(json.loads(result.stderr), {"error": "login_requires_terminal"})
            profile.chmod(0o755)
            result = run("agent-login", *args, env=env)
            self.assertEqual(json.loads(result.stderr), {"error": "profile_unsafe"})

    def test_terminal_is_detached_into_its_own_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = self.fake_bundle(root, "codex")
            tools = root / "tools"
            tools.mkdir()
            home = root / "home"
            home.mkdir()
            profile = root / "state/codex"
            env = {"HOME": str(home), "PATH": str(tools)}
            args = ("codex", "--dry-run", "--bundle", str(bundle), "--profile", str(profile))
            for name in ("xdg-terminal-exec", "setsid", "systemd-run"):
                (tools / name).write_text("#!/bin/sh\nexit 97\n")
                (tools / name).chmod(0o755)
            inner = ["/usr/bin/python3", str(SCRIPTS.resolve() / "agent-login"), "--in-terminal", "codex",
                     "--bundle", str(bundle), "--profile", str(profile)]
            terminal = [str(tools / "xdg-terminal-exec"), "--title=Buzz agent sign-in", "-e", *inner]
            result = run("agent-login", *args, env=env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout),
                             [str(tools / "systemd-run"), "--user", "--scope", "--collect", "--quiet", "--",
                              *terminal])
            (tools / "systemd-run").unlink()
            self.assertEqual(json.loads(run("agent-login", *args, env=env).stdout),
                             [str(tools / "setsid"), "-f", *terminal])
            floating = tools / login_tool.FLOATING
            floating.write_text("#!/bin/sh\n")
            floating.chmod(0o755)
            self.assertEqual(json.loads(run("agent-login", *args, env=env).stdout),
                             [str(tools / "setsid"), "-f", str(floating), shlex.join(inner)])
            # A real launch runs the detach argv and does not wait for the terminal.
            recorder = tools / "systemd-run"
            record = root / "argv.json"
            recorder.write_text("#!/usr/bin/python3\nimport json, sys\nopen(%r, 'w').write(json.dumps(sys.argv[1:]))\n"
                                % str(record))
            recorder.chmod(0o755)
            launch = [a for a in args if a != "--dry-run"]
            result = run("agent-login", *launch, env=env)
            self.assertEqual((result.returncode, result.stdout), (0, "launched\n"), result.stderr)
            for _ in range(200):
                if record.exists() and record.read_text():
                    break
                time.sleep(0.05)
            self.assertEqual(json.loads(record.read_text()),
                             ["--user", "--scope", "--collect", "--quiet", "--", str(floating), shlex.join(inner)])
            # Refusal on an unknown harness is unchanged and launches nothing.
            record.unlink()
            result = run("agent-login", "goose", env=env)
            self.assertEqual((result.returncode, json.loads(result.stderr)), (2, {"error": "harness_unknown"}))
            self.assertFalse(record.exists())


if __name__ == "__main__":
    unittest.main()
