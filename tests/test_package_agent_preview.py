#!/usr/bin/env python3
"""Offline checks for native preview package boundaries."""
import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import re
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/package-agent-preview"
loader = importlib.machinery.SourceFileLoader("package_agent_preview", str(SCRIPT))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)


class PackageAgentPreview(unittest.TestCase):
    def test_ordinary_publication_matches_arm64_build_patches(self):
        workflow = (module.ROOT / ".github/workflows/agent-arm64.yml").read_text()
        applied = re.search(r"for patch in ([^;\n]+); do", workflow)
        self.assertIsNotNone(applied)
        expected = ("ws-resource-limits", "acp-permission-mode", "acp-interactive-login",
                    "acp-key-isolation", "acp-harness-replies", "acp-deny-tool-requests",
                    "acp-auth-timeout")
        self.assertEqual(tuple(applied.group(1).split()), expected)
        self.assertEqual(module.PATCHES["buzz"], expected)
        patches = [{"name": name, "sha256": "fixture"} for name in applied.group(1).split()]
        metadata = module.publication_metadata(patches)
        self.assertEqual(metadata["publicationContract"], "ordinary")
        self.assertEqual(metadata["relayRequirement"], {
            "publicationRoute": "/events", "verifiedOnTargetRelay": False,
            "agentServiceAllowed": False})
        with self.assertRaises(ValueError):
            module.publication_metadata(patches + [{"name": "member-bound-events"}])
        with self.assertRaises(ValueError):
            module.publication_metadata(patches[:-1])

    def test_arm64_elf_required(self):
        binary = bytearray(64)
        binary[:7] = b"\x7fELF\x02\x01\x01"
        binary[16:18] = (3).to_bytes(2, "little")
        binary[18:20] = (183).to_bytes(2, "little")
        module.elf_arm64(binary)
        binary[18:20] = (62).to_bytes(2, "little")
        with self.assertRaises(ValueError):
            module.elf_arm64(binary)

    def test_native_probe_hashes_and_symlink_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "node_modules/@openai/codex-linux-arm64"
            package.mkdir(parents=True)
            binary = package / "codex"
            binary.write_bytes(b"native-arm64-fixture")
            result = module.native_probe_files(root, "@openai/codex-linux-arm64")
            self.assertEqual(result["node_modules/@openai/codex-linux-arm64/codex"],
                             hashlib.sha256(binary.read_bytes()).hexdigest())
            (package / "unexpected-link").symlink_to(binary)
            with self.assertRaises(ValueError):
                module.native_probe_files(root, "@openai/codex-linux-arm64")

    def test_runtime_tree_omits_npm_shims_and_rejects_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".bin").mkdir()
            (root / ".bin/npm").symlink_to("../bin/npm-cli.js")
            (root / "bin").mkdir()
            (root / "bin/npm-cli.js").write_text("fixture")
            members = {}
            module.add_tree(members, root, "runtime/npm")
            self.assertEqual(members, {"runtime/npm/bin/npm-cli.js": b"fixture"})
            (root / "bin/unexpected-link").symlink_to("npm-cli.js")
            with self.assertRaises(ValueError):
                module.add_tree({}, root, "runtime/npm")

    def test_rust_notices_match_lock_and_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "texts").mkdir()
            lock = b"pinned-lock-fixture"
            content = b"fixture license text"
            digest = hashlib.sha256(content).hexdigest()
            (root / "texts" / (digest + ".txt")).write_bytes(content)
            (root / "README.txt").write_text("preview inventory")
            document = {"schemaVersion": 1, "target": "aarch64-unknown-linux-gnu",
                        "lockSha256": hashlib.sha256(lock).hexdigest(), "reviewRequired": True,
                        "packages": [{"reviewFlags": ["needs_review"], "licenseFiles": [
                            {"path": "texts/" + digest + ".txt", "sha256": digest,
                             "bytes": len(content)}]}]}
            (root / "inventory.json").write_text(json.dumps(document))
            members, summary = module.rust_notice_members(root, lock)
            self.assertEqual(summary["flaggedPackages"], 1)
            self.assertIn("notices/rust/texts/" + digest + ".txt", members)
            with self.assertRaises(ValueError):
                module.rust_notice_members(root, b"other-lock")
            (root / "texts" / (digest + ".txt")).write_text("changed")
            with self.assertRaises(ValueError):
                module.rust_notice_members(root, lock)


if __name__ == "__main__":
    unittest.main()
