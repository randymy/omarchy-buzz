#!/usr/bin/env python3
"""Offline tests for scripts/verify-package: synthetic ELF headers, never executed."""
import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
VERSION = json.loads((ROOT / "manifest.json").read_text())["version"]
REVISION = "781d39510cf23cfe224e8f521ae06a23377e06de"


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


verifier = load("verify_package", ROOT / "scripts/verify-package")
packager = verifier.packager


def notices_directory(path, arch):
    """A minimal inventory over one real locked package, as helper-notices writes it."""
    lock = (ROOT / "helper/Cargo.lock").read_bytes()
    package = next(p for p in tomllib.loads(lock.decode())["package"] if p.get("checksum"))
    text = b"Synthetic notice text\n"
    sha = hashlib.sha256(text).hexdigest()
    (path / "texts").mkdir(parents=True)
    (path / "texts" / (sha + ".txt")).write_bytes(text)
    (path / "README.txt").write_text("synthetic\n")
    inventory = {"schemaVersion": 1, "reviewRequired": True, "target": arch + "-unknown-linux-gnu",
                 "lockSha256": hashlib.sha256(lock).hexdigest(),
                 "packages": [{"name": package["name"], "version": package["version"],
                               "source": package.get("source"), "registryChecksum": package["checksum"],
                               "reviewFlags": [], "licenseFiles": [{"path": "texts/" + sha + ".txt",
                                                                    "sha256": sha, "bytes": len(text)}]}]}
    (path / "inventory.json").write_text(json.dumps(inventory))


class VerifyPackage(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="verify package ")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.artifacts = {arch: self.artifact(arch, machine) for arch, machine in (("aarch64", 183), ("x86_64", 62))}

    def artifact(self, arch, machine):
        """Lay out a directory exactly as the helper build workflow uploads it."""
        directory = self.base / ("helper-" + arch)
        directory.mkdir()
        data = bytearray(64)
        data[:7] = b"\x7fELF\x02\x01\x01"
        data[16:18] = (3).to_bytes(2, "little")
        data[18:20] = machine.to_bytes(2, "little")
        binary = directory / "omarchy-buzz"
        binary.write_bytes(data)
        notices = directory / "notices"
        notices_directory(notices, arch)
        metadata = {"helperVersion": VERSION, "protocolVersion": 1, "backendRevision": REVISION}
        packager.package(binary, directory, metadata_runner=lambda _: metadata, notices=notices)
        build = {"sourceRevision": "a" * 40, "runId": "1", "architecture": arch, "developmentPreview": True,
                 "sha256": hashlib.sha256(data).hexdigest(), "metadata": metadata, "dynamicLibraries": []}
        (directory / "build.json").write_text(json.dumps(build))
        shutil.copy2(ROOT / "LICENSE", directory / "LICENSE")
        return directory

    def test_both_architectures_verify_on_any_host(self):
        for arch, directory in self.artifacts.items():
            with self.subTest(arch=arch):
                summary = verifier.verify(directory, arch, source_revision="a" * 40)
                self.assertEqual(summary["archive"], f"omarchy-buzz-{VERSION}-linux-{arch}.tar.gz")
                self.assertEqual(summary["noticePackages"], 1)

    def test_wrong_architecture_and_revision_rejected(self):
        with self.assertRaises(ValueError):
            verifier.verify(self.artifacts["aarch64"], "x86_64")
        with self.assertRaisesRegex(ValueError, "different source revision"):
            verifier.verify(self.artifacts["aarch64"], "aarch64", source_revision="b" * 40)

    def test_build_json_must_describe_the_packaged_binary(self):
        directory = self.artifacts["x86_64"]
        path = directory / "build.json"
        original = json.loads(path.read_text())
        for change in ({"sha256": "0" * 64}, {"architecture": "aarch64"},
                       {"metadata": original["metadata"] | {"helperVersion": "9.9.9"}},
                       {"sourceRevision": "main"}):
            with self.subTest(change=change):
                path.write_text(json.dumps(original | change))
                with self.assertRaises(ValueError):
                    verifier.verify(directory, "x86_64")
        path.write_text("[]")
        with self.assertRaises(ValueError):
            verifier.verify(directory, "x86_64")

    def test_tampered_archive_binary_license_or_notices_rejected(self):
        directory = self.artifacts["aarch64"]
        archive = directory / f"omarchy-buzz-{VERSION}-linux-aarch64.tar.gz"
        cases = [
            (archive, lambda data: data[:-1] + bytes([data[-1] ^ 1])),
            (directory / "omarchy-buzz", lambda data: data + b"\0"),
            (directory / "LICENSE", lambda data: data + b"\n"),
            (directory / "notices/README.txt", lambda data: b"changed\n"),
        ]
        for path, change in cases:
            with self.subTest(path=path.name):
                original = path.read_bytes()
                path.write_bytes(change(original))
                with self.assertRaises(ValueError):
                    verifier.verify(directory, "aarch64")
                path.write_bytes(original)
        verifier.verify(directory, "aarch64")

    def test_package_without_notices_is_not_releasable(self):
        directory = self.base / "bare"
        directory.mkdir()
        source = self.artifacts["aarch64"]
        metadata = {"helperVersion": VERSION, "protocolVersion": 1, "backendRevision": REVISION}
        packager.package(source / "omarchy-buzz", directory, metadata_runner=lambda _: metadata)
        for name in ("build.json", "LICENSE"):
            shutil.copy2(source / name, directory / name)
        shutil.copytree(source / "notices", directory / "notices")
        with self.assertRaisesRegex(ValueError, "must include third-party notices"):
            verifier.verify(directory, "aarch64")


if __name__ == "__main__":
    unittest.main()
