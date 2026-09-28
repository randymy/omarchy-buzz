#!/usr/bin/env python3
"""Offline package tests: synthetic ELF headers never executed."""
import hashlib
import importlib.machinery
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
loader = importlib.machinery.SourceFileLoader("package_helper", str(ROOT / "scripts/package-helper"))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)


class Packaging(unittest.TestCase):
    def test_reproducibility_architecture_and_checksum(self):
        with tempfile.TemporaryDirectory(prefix="buzz package spaces ") as temporary:
            base = Path(temporary)
            for machine, arch in [(62, "x86_64"), (183, "aarch64")]:
                binary = base / ("fake binary " + arch)
                data = bytearray(64)
                data[:7] = b"\x7fELF\x02\x01\x01"
                data[16:18] = (3).to_bytes(2, "little")
                data[18:20] = machine.to_bytes(2, "little")
                binary.write_bytes(data)
                metadata = {"helperVersion": "0.0.4", "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
                first, digest = module.package(binary, base / (arch + " one"), metadata_runner=lambda _: metadata)
                second, _ = module.package(binary, base / (arch + " two"), metadata_runner=lambda _: metadata)
                self.assertEqual(first.read_bytes(), second.read_bytes())
                manifest = json.loads(digest.read_bytes())
                self.assertEqual(manifest["artifacts"][0]["sha256"], hashlib.sha256(first.read_bytes()).hexdigest())
                self.assertEqual(manifest["artifacts"][0]["bytes"], first.stat().st_size)
                with tarfile.open(fileobj=io.BytesIO(first.read_bytes()), mode="r:gz") as tar:
                    names = tar.getnames()
                    self.assertEqual(len(names), 7)
                    self.assertTrue(all("config" not in name for name in names))
                    version = json.load(tar.extractfile(next(name for name in names if name.endswith("version.json"))))
                    self.assertEqual(version["architecture"], arch)
                    executable = next(item for item in tar.getmembers() if item.name.endswith("bin/omarchy-buzz"))
                    self.assertEqual(executable.mode, 0o755)
                    self.assertEqual(executable.mtime, 0)
                with self.assertRaises(ValueError):
                    module.package(binary, first.parent, metadata_runner=lambda _: metadata)

    def test_bad_metadata_and_elf_rejected(self):
        with self.assertRaises(ValueError):
            module.architecture(b"not ELF")
        data = bytearray(64)
        data[:7] = b"\x7fELF\x02\x01\x01"
        data[16:18] = (2).to_bytes(2, "little")
        data[18:20] = (40).to_bytes(2, "little")
        with self.assertRaises(ValueError):
            module.architecture(data)
        good = {"helperVersion": "0.0.4", "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
        for changed in ({"helperVersion": "0.0.2"}, {"backendRevision": "0" * 40}, {"protocolVersion": True}):
            with self.assertRaises(ValueError):
                module.validated_metadata(ROOT, good | changed)

    def test_source_version_and_dependency_pin_mismatch(self):
        good = {"helperVersion": "0.0.4", "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("manifest.json", "helper/Cargo.toml", "helper/src/compatibility.rs"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((ROOT / name).read_bytes())
            module.validated_metadata(root, good)
            cargo = root / "helper/Cargo.toml"
            cargo.write_text(cargo.read_text().replace(good["backendRevision"], "0" * 40, 1))
            with self.assertRaises(ValueError):
                module.validated_metadata(root, good)
            cargo.write_bytes((ROOT / "helper/Cargo.toml").read_bytes())
            manifest = json.loads((root / "manifest.json").read_text())
            manifest["version"] = "0.0.2"
            (root / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                module.validated_metadata(root, good)


if __name__ == "__main__":
    unittest.main()
