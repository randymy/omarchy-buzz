"""Offline boundary checks for the opt-in agent authentication preview."""
import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/agent-preview"
loader = importlib.machinery.SourceFileLoader("agent_preview", str(SCRIPT))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)


def fixture_bundle(root):
    bundle = root / "bundle"
    bundle.mkdir()
    files = {}
    for name in module.ENTRIES.values():
        path = bundle / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(name.encode())
        files[name] = {"bytes": path.stat().st_size,
                       "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    report = {
        "schemaVersion": 1,
        "entrypoints": module.ENTRIES,
        "guardArguments": module.GUARDS,
        "platform": {"os": "linux", "architecture": "aarch64", "libc": "glibc"},
        "preview": {"experimental": True, "releaseReady": False},
        "sources": {name: {"revision": revision} for name, revision in module.PINS.items()},
        "files": files,
    }
    (bundle / "manifest.json").write_text(json.dumps(report))
    return bundle, report


class AgentPreview(unittest.TestCase):
    def test_bundle_rejects_file_change_and_linked_entrypoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle, _ = fixture_bundle(Path(temporary))
            with patch.object(module.platform, "machine", return_value="aarch64"):
                module.verify_bundle(bundle, module.digest(bundle / 'manifest.json'))
                entry = bundle / module.ENTRIES["buzzAcp"]
                entry.write_bytes(b"changed")
                with self.assertRaisesRegex(module.Refused, "bundle_hash_mismatch"):
                    module.verify_bundle(bundle, module.digest(bundle / 'manifest.json'))
                entry.unlink()
                entry.symlink_to(bundle / module.ENTRIES["node"])
                with self.assertRaisesRegex(module.Refused, "linked_bundle_file"):
                    module.verify_bundle(bundle, module.digest(bundle / 'manifest.json'))

    def test_profile_creation_and_existing_unsafe_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(module.Path, "home", return_value=root), patch.object(module.os, "umask", return_value=0o077):
                profile = module.profile_for("codex")
                self.assertEqual(profile.stat().st_mode & 0o777, 0o700)
                self.assertEqual((profile / "provider").stat().st_mode & 0o777, 0o700)
                (profile / "provider").chmod(0o755)
                with self.assertRaisesRegex(module.Refused, "profile_permissions_unsafe"):
                    module.profile_for("codex")

    def test_reused_profile_rejects_linked_config_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(module.Path, "home", return_value=root):
                profile = module.profile_for("claude")
                (profile / "provider" / "settings.json").symlink_to(root / "outside")
                with self.assertRaisesRegex(module.Refused, "profile_tree_unsafe"):
                    module.profile_for("claude")

    def test_manifest_requires_independent_expected_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle, _ = fixture_bundle(Path(temporary))
            with self.assertRaisesRegex(module.Refused, "trusted_manifest_hash_required"):
                module.verify_bundle(bundle, "0" * 64)

    def test_discovery_output_is_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            child = root / 'peer.py'
            child.write_text('import sys\nsys.stdout.write("X"*70000)\nsys.stdout.flush()\n')
            with self.assertRaisesRegex(module.Refused, 'auth_discovery_output_limit'):
                module.capture_discovery([sys.executable, str(child)], {}, root)

    def test_login_commands_select_only_subscription_methods(self):
        for agent, method in [('codex', 'chat-gpt'), ('claude', 'claude-ai-login')]:
            command = module.auth_command(Path('/verified'), agent, login=True)
            self.assertEqual(command[1], 'authenticate')
            self.assertEqual(command[-2:], ['--method-id', method])
            self.assertIn(module.GUARDS[agent], command[5])

    def test_environment_excludes_ambient_credentials_and_options(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile = root / "profile"
            bundle = root / "bundle"
            ambient = {"TERM": "xterm-256color", "DISPLAY": ":1",
                       "DBUS_SESSION_BUS_ADDRESS": "unix:path=/run/user/test/bus",
                       "BUZZ_PRIVATE_KEY": "secret", "OPENAI_API_KEY": "secret",
                       "ANTHROPIC_API_KEY": "secret", "NODE_OPTIONS": "--require evil",
                       "HTTP_PROXY": "http://proxy", "CODEX_HOME": "/normal/profile"}
            env = module.environment(profile, bundle, ambient)
            self.assertEqual(env["HOME"], str(profile / "home"))
            self.assertEqual(env["DISPLAY"], ":1")
            self.assertFalse(set(env) & {"BUZZ_PRIVATE_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY",
                                         "NODE_OPTIONS", "HTTP_PROXY", "CODEX_HOME"})

    def test_runtime_probe_detects_native_file_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle, report = fixture_bundle(root)
            native = bundle / "adapters/codex/node_modules/@openai/codex-linux-arm64/codex"
            native.parent.mkdir(parents=True)
            native.write_bytes(b"native")
            package = native.parent / "package.json"
            package.write_text('{"version":"0.158.0-linux-arm64"}')
            report["npmProbePackages"] = {"codex": [{"path": "node_modules/@openai/codex-linux-arm64",
                                                         "version": "0.158.0-linux-arm64"}]}
            report["nativeProbeFiles"] = {"codex": {"node_modules/@openai/codex-linux-arm64/codex":
                                                        hashlib.sha256(b"native").hexdigest()}}
            module.verify_runtime(bundle, report, "codex")
            native.write_bytes(b"different")
            with self.assertRaisesRegex(module.Refused, "native_runtime_hash_mismatch"):
                module.verify_runtime(bundle, report, "codex")


if __name__ == "__main__":
    unittest.main()
