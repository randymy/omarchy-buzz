#!/usr/bin/env python3
"""Offline notice inventory and archive-boundary tests."""
import importlib.machinery
import importlib.util
import json
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    loader = importlib.machinery.SourceFileLoader(name.replace("-", "_"), str(ROOT / "scripts" / name))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    result = importlib.util.module_from_spec(spec)
    loader.exec_module(result)
    return result


notices = load("helper-notices")
packaging = load("package-helper")


class Notices(unittest.TestCase):
    def test_pinned_archive_tag_evidence_and_tampering(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            metadata, lock = self.fixture(base)
            (base / "dep/LICENSE").unlink()
            (base / "dep/vendor/NOTICE").unlink()
            source = base / "dep/src/lib.rs"
            source.parent.mkdir()
            source.write_bytes(b"// SPDX-License-Identifier: CC0-1.0\n")
            supplemental = base / "supplemental"
            supplemental.mkdir()
            payload = b"Exact tagged root license\n"
            (supplemental / "LICENSE.txt").write_bytes(payload)
            revision = "a" * 40
            upstream = "https://raw.githubusercontent.com/example/dep/" + revision + "/"
            item = {"name": "dep", "version": "1.0.0", "source": metadata["packages"][1]["source"],
                    "registryChecksum": "a" * 64, "archiveSha256": "a" * 64,
                    "manifestSha256": hashlib.sha256((base / "dep/Cargo.toml").read_bytes()).hexdigest(),
                    "tagName": "dep-1.0.0", "revision": revision,
                    "sourceUrl": upstream + "LICENSE", "sourcePath": "LICENSE",
                    "matchedSourcePath": "src/lib.rs", "matchedSourceRepoPath": "dep/src/lib.rs",
                    "matchedSourceUrl": upstream + "dep/src/lib.rs",
                    "matchedSourceSha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                    "file": "LICENSE.txt", "sha256": hashlib.sha256(payload).hexdigest(), "bytes": len(payload)}
            document = {"schemaVersion": 1, "packages": [item]}
            (supplemental / "evidence.json").write_text(json.dumps(document))
            inventory, texts = notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            package = inventory["packages"][0]
            self.assertEqual(texts[hashlib.sha256(payload).hexdigest()], payload)
            self.assertIn("supplemental_notice_review", package["reviewFlags"])
            self.assertEqual(package["licenseFiles"][0]["supplementalEvidence"]["tagName"], "dep-1.0.0")
            source.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "matched source mismatch"):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            source.write_bytes(b"// SPDX-License-Identifier: CC0-1.0\n")
            item["archiveSha256"] = "b" * 64
            (supplemental / "evidence.json").write_text(json.dumps(document))
            with self.assertRaisesRegex(ValueError, "archive/tag evidence"):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)

    def test_pinned_supplemental_evidence_and_tampering(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            metadata, lock = self.fixture(base)
            (base / "dep/LICENSE").unlink()
            (base / "dep/vendor/NOTICE").unlink()
            vcs = b'{"git":{"sha1":"' + b"a" * 40 + b'"},"path_in_vcs":"crates/dep"}'
            (base / "dep/.cargo_vcs_info.json").write_bytes(vcs)
            supplemental = base / "supplemental"
            supplemental.mkdir()
            payload = b"Exact synthetic upstream notice\n"
            (supplemental / "LICENSE.txt").write_bytes(payload)
            item = {"name": "dep", "version": "1.0.0", "source": metadata["packages"][1]["source"],
                    "registryChecksum": "a" * 64,
                    "manifestSha256": hashlib.sha256((base / "dep/Cargo.toml").read_bytes()).hexdigest(),
                    "revision": "a" * 40, "pathInVcs": "crates/dep",
                    "vcsInfoSha256": hashlib.sha256(vcs).hexdigest(),
                    "sourceUrl": "https://raw.githubusercontent.com/example/dep/" + "a" * 40 + "/LICENSE",
                    "sourcePath": "LICENSE", "file": "LICENSE.txt",
                    "sha256": hashlib.sha256(payload).hexdigest(), "bytes": len(payload)}
            evidence = {"schemaVersion": 1, "packages": [item]}
            (supplemental / "evidence.json").write_text(json.dumps(evidence))
            document, texts = notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            package = document["packages"][0]
            self.assertIn("supplemental_notice_review", package["reviewFlags"])
            self.assertNotIn("no_notice_text_found", package["reviewFlags"])
            self.assertEqual(list(texts.values()), [payload])
            self.assertEqual(package["licenseFiles"][0]["supplementalEvidence"]["revision"], "a" * 40)
            with patch.object(notices, "MAX_TOTAL", len(payload) - 1):
                with self.assertRaisesRegex(ValueError, "bounded budget"):
                    notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            (supplemental / "LICENSE.txt").write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            (supplemental / "evidence.json").write_text("[]")
            with self.assertRaisesRegex(ValueError, "unsupported supplemental evidence"):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            (supplemental / "LICENSE.txt").write_bytes(payload)
            item["sourcePath"] = "../LICENSE"
            (supplemental / "evidence.json").write_text(json.dumps(evidence))
            with self.assertRaises(ValueError):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            item["sourcePath"] = "LICENSE"
            (supplemental / "evidence.json").write_text(json.dumps(evidence))
            (base / "dep/.cargo_vcs_info.json").write_bytes(b"changed")
            with self.assertRaises(ValueError):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)
            (base / "dep/.cargo_vcs_info.json").write_bytes(vcs)
            item["registryChecksum"] = "b" * 64
            (supplemental / "evidence.json").write_text(json.dumps(evidence))
            with self.assertRaises(ValueError):
                notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu", supplemental)

    def fixture(self, base):
        packages = []
        for name, source in (("helper", None), ("dep", "registry+https://example.invalid/index"), ("dev", "registry+https://example.invalid/index")):
            folder = base / name
            folder.mkdir()
            (folder / "Cargo.toml").write_text('[package]\nname="' + name + '"\nversion="1.0.0"\n')
            packages.append({"id": name, "name": name, "version": "1.0.0", "source": source,
                             "manifest_path": str(folder / "Cargo.toml"), "license": "MIT", "license_file": None})
        (base / "dep/LICENSE").write_bytes(b"Synthetic license notice\n")
        (base / "dep/vendor").mkdir()
        (base / "dep/vendor/NOTICE").write_bytes(b"Synthetic vendored notice\n")
        metadata = {"packages": packages, "resolve": {"root": "helper", "nodes": [
            {"id": "helper", "deps": [{"pkg": "dep", "dep_kinds": [{"kind": None}]}, {"pkg": "dev", "dep_kinds": [{"kind": "dev"}]}]},
            {"id": "dep", "deps": []}, {"id": "dev", "deps": []}]}}
        lock = 'version=4\n' + ''.join('[[package]]\nname="' + p["name"] + '"\nversion="1.0.0"\n' + ('source="' + p["source"] + '"\nchecksum="' + 'a' * 64 + '"\n' if p["source"] else '') for p in packages)
        return metadata, lock.encode()

    def test_reproducible_original_bytes_provenance_and_dev_scope(self):
        with tempfile.TemporaryDirectory(prefix="notices spaces ") as temp:
            base = Path(temp)
            metadata, lock = self.fixture(base)
            first, texts = notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu")
            second, again = notices.inventory(metadata, lock, "aarch64-unknown-linux-gnu")
            self.assertEqual(first, second)
            self.assertEqual(texts, again)
            self.assertEqual([p["name"] for p in first["packages"]], ["dep"])
            self.assertEqual(first["packages"][0]["registryChecksum"], "a" * 64)
            self.assertEqual(len(first["packages"][0]["licenseFiles"]), 2)
            self.assertIn(b"Synthetic vendored notice\n", texts.values())
            self.assertNotIn(str(base), json.dumps(first))
            self.assertTrue(first["reviewRequired"])

    def test_missing_notice_symlink_and_lock_mismatch(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            metadata, lock = self.fixture(base)
            (base / "dep/LICENSE").unlink()
            (base / "dep/vendor/NOTICE").unlink()
            (base / "dep/LICENSE").symlink_to(base / "helper/Cargo.toml")
            document, texts = notices.inventory(metadata, lock, "x86_64-unknown-linux-gnu")
            self.assertFalse(texts)
            self.assertIn("no_notice_text_found", document["packages"][0]["reviewFlags"])
            self.assertTrue(any("symlink" in flag for flag in document["packages"][0]["reviewFlags"]))
            with self.assertRaises(ValueError):
                notices.inventory(metadata, lock.replace(b'name="dep"', b'name="other"'), "x86_64-unknown-linux-gnu")

    def test_package_accepts_only_hash_and_lock_matched_notices(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            metadata, lock = self.fixture(base)
            metadata_path = base / "metadata.json"
            metadata_path.write_text(json.dumps(metadata))
            (base / "helper/Cargo.lock").write_bytes(lock)
            output = base / "output"
            document = notices.generate(metadata_path, base / "helper/Cargo.lock", "aarch64-unknown-linux-gnu", output)
            members, report = packaging.notice_members(output, base, "aarch64")
            self.assertTrue(report["included"])
            self.assertEqual(report["flaggedPackages"], 0)
            self.assertTrue(all(name.startswith("third-party/") for name in members))
            with self.assertRaises(ValueError):
                packaging.notice_members(output, base, "x86_64")
            path = output / document["packages"][0]["licenseFiles"][0]["path"]
            original = path.read_bytes()
            path.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                packaging.notice_members(output, base, "aarch64")
            path.write_bytes(original)
            (base / "helper/Cargo.lock").write_bytes(lock + b"\n")
            with self.assertRaises(ValueError):
                packaging.notice_members(output, base, "aarch64")


if __name__ == "__main__":
    unittest.main()
