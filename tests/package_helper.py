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
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
VERSION = json.loads((ROOT / "manifest.json").read_text())["version"]
def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


module = load("package_helper", ROOT / "scripts/package-helper")


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
                metadata = {"helperVersion": VERSION, "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
                first, digest = module.package(binary, base / (arch + " one"), metadata_runner=lambda _: metadata)
                second, _ = module.package(binary, base / (arch + " two"), metadata_runner=lambda _: metadata)
                self.assertEqual(first.read_bytes(), second.read_bytes())
                manifest = json.loads(digest.read_bytes())
                self.assertEqual(manifest["artifacts"][0]["sha256"], hashlib.sha256(first.read_bytes()).hexdigest())
                self.assertEqual(manifest["artifacts"][0]["bytes"], first.stat().st_size)
                with tarfile.open(fileobj=io.BytesIO(first.read_bytes()), mode="r:gz") as tar:
                    names = tar.getnames()
                    self.assertEqual(len(names), 18)
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
        good = {"helperVersion": VERSION, "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
        for changed in ({"helperVersion": "0.0.2"}, {"backendRevision": "0" * 40}, {"protocolVersion": True}):
            with self.assertRaises(ValueError):
                module.validated_metadata(ROOT, good | changed)

    def test_source_version_and_dependency_pin_mismatch(self):
        good = {"helperVersion": VERSION, "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
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

    def test_ws_client_pin_source_and_constant(self):
        """The ws-client pin must be a revision of an allowed source and equal
        WS_CLIENT_REVISION; the installer applies the same rule."""
        good = {"helperVersion": VERSION, "protocolVersion": 1, "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
        installer = load("helper_install_pins", ROOT / "scripts/helper-install")
        self.assertEqual(installer.BUZZ_SOURCES, module.BUZZ_SOURCES)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("manifest.json", "helper/Cargo.toml", "helper/src/compatibility.rs"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((ROOT / name).read_bytes())
            cargo = root / "helper/Cargo.toml"
            original = cargo.read_text()
            ws_line = next(line for line in original.splitlines() if line.startswith("buzz-ws-client = "))
            ws_revision = ws_line.split('rev = "')[1].split('"')[0]
            for broken in (ws_line.replace(ws_revision, "0" * 40),
                           ws_line.replace("github.com/randymy/buzz", "github.com/someone-else/buzz"),
                           ws_line.replace('rev = "' + ws_revision + '"', 'branch = "main"')):
                cargo.write_text(original.replace(ws_line, broken, 1))
                with self.assertRaises((ValueError, KeyError)):
                    module.validated_metadata(root, good)
                with self.assertRaises((ValueError, KeyError)):
                    installer.check_buzz_pins(root, tomllib.loads(cargo.read_text()))
            cargo.write_text(original)
            self.assertEqual(module.check_buzz_pins(root, tomllib.loads(original)),
                             (good["backendRevision"], ws_revision))


if __name__ == "__main__":
    unittest.main()
