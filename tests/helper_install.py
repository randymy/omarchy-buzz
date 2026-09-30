#!/usr/bin/env python3
"""Offline installer tests; systemd is replaced and synthetic ELF is never run."""
import hashlib
import importlib.machinery
import importlib.util
import io
import json
from pathlib import Path
import platform
import subprocess
import tempfile
import tarfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


installer = load("helper_install", ROOT / "scripts/helper-install")
packager = load("package_helper_installer_test", ROOT / "scripts/package-helper")


class HelperInstall(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.home = self.base / "home"
        self.home.mkdir()
        machine = {"aarch64": 183, "arm64": 183, "x86_64": 62}[platform.machine()]
        binary = self.base / "binary"
        data = bytearray(64)
        data[:7] = b"\x7fELF\x02\x01\x01"
        data[16:18] = (3).to_bytes(2, "little")
        data[18:20] = machine.to_bytes(2, "little")
        binary.write_bytes(data)
        version = json.loads((ROOT / "manifest.json").read_text())["version"]
        metadata = {"helperVersion": version, "protocolVersion": 1,
                    "backendRevision": "781d39510cf23cfe224e8f521ae06a23377e06de"}
        self.version_probe = patch.object(installer.subprocess, "run", return_value=subprocess.CompletedProcess(
            [], 0, json.dumps(metadata).encode(), b""))
        self.version_run = self.version_probe.start()
        self.addCleanup(self.version_probe.stop)
        self.archive, self.sidecar = packager.package(
            binary, self.base / "package", metadata_runner=lambda _: metadata)
        self.calls = []
        self.systemd = patch.object(installer, "systemctl", side_effect=self.fake_systemctl)
        self.systemd.start()
        self.addCleanup(self.systemd.stop)

    def fake_systemctl(self, *args, **kwargs):
        self.calls.append(args)
        return args[0] in ("is-enabled", "is-active")

    def paths(self):
        return installer.destinations(self.home)

    def test_install_upgrade_uninstall_and_preserved_backup(self):
        installer.install(self.home, self.archive, self.sidecar, dry_run=True)
        self.assertFalse((self.home / ".local").exists())
        self.assertEqual(self.calls, [])
        installer.install(self.home, self.archive, self.sidecar)
        self.assertTrue(any(call.kwargs.get("timeout") == 10 and "cwd" in call.kwargs
                            for call in self.version_run.call_args_list))
        self.assertTrue(all(p.is_file() for p in self.paths().values()))
        installer.install(self.home, self.archive, self.sidecar)
        backups = self.home / ".local/share/omarchy-buzz/backups"
        self.assertEqual(len(list(backups.iterdir())), 1)
        installer.uninstall(self.home, dry_run=True)
        self.assertTrue(all(p.is_file() for p in self.paths().values()))
        installer.uninstall(self.home)
        self.assertTrue(all(not p.exists() for p in self.paths().values()))
        self.assertEqual(len(list(backups.iterdir())), 2)
        self.assertIn(("disable", "--now", "omarchy-buzz.socket"), self.calls)

    def test_upgrade_from_install_without_agent_files(self):
        paths = self.paths()
        installer.install(self.home, self.archive, self.sidecar)
        for name in ("service/omarchy-buzz-agents.service", "service/omarchy-buzz-agents.socket", "scripts/agent-login"):
            paths[name].unlink()
        self.calls.clear()
        installer.install(self.home, self.archive, self.sidecar)
        self.assertTrue(all(path.exists() for path in paths.values()))
        self.assertIn(("enable", "--now", "omarchy-buzz-agents.socket"), self.calls)
        self.assertNotIn(("stop", "omarchy-buzz-agents.socket", "omarchy-buzz-agents.service"), self.calls)
        paths["scripts/agent-login"].unlink()
        paths["bin/omarchy-buzz"].unlink()
        with self.assertRaisesRegex(ValueError, "partial installation"):
            installer.inspect_existing(paths)

    def test_upgrade_accepts_its_own_recorded_copy_but_not_an_edit(self):
        paths = self.paths()
        installer.install(self.home, self.archive, self.sidecar)
        record = json.loads((self.home / installer.RECORD).read_text())
        self.assertEqual(set(record), set(installer.REVIEWED))
        # The checkout's script moves on after this install: the installed
        # copy no longer matches it but is the one this installer recorded.
        older = b"#!/usr/bin/python3\n# an earlier release of this script\n"
        paths["scripts/room-agent"].write_bytes(older)
        record["scripts/room-agent"] = hashlib.sha256(older).hexdigest()
        (self.home / installer.RECORD).write_text(json.dumps(record))
        self.calls.clear()
        installer.install(self.home, self.archive, self.sidecar)
        self.assertEqual(paths["scripts/room-agent"].read_bytes(), (ROOT / "scripts/room-agent").read_bytes())
        self.assertEqual(json.loads((self.home / installer.RECORD).read_text())["scripts/room-agent"],
                         hashlib.sha256((ROOT / "scripts/room-agent").read_bytes()).hexdigest())
        backups = sorted((self.home / ".local/share/omarchy-buzz/backups").iterdir())
        self.assertEqual((backups[-1] / "scripts/room-agent").read_bytes(), older)
        self.assertTrue((backups[-1] / "installed.json").is_file())
        # An edit nobody recorded is still refused, and a damaged record is ignored.
        paths["scripts/room-agent"].write_bytes(b"edited by hand\n")
        with self.assertRaisesRegex(ValueError, "modified or unrecognized"):
            installer.install(self.home, self.archive, self.sidecar)
        (self.home / installer.RECORD).write_text("{not json")
        self.assertEqual(installer.read_record(self.home), {})
        with self.assertRaisesRegex(ValueError, "modified or unrecognized"):
            installer.install(self.home, self.archive, self.sidecar)
        paths["scripts/room-agent"].write_bytes((ROOT / "scripts/room-agent").read_bytes())
        installer.uninstall(self.home)
        self.assertFalse((self.home / installer.RECORD).exists())

    def test_checksum_and_unexpected_unit_rejected_before_systemd(self):
        data = json.loads(self.sidecar.read_text())
        data["artifacts"][0]["sha256"] = "0" * 64
        self.sidecar.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, "checksum"):
            installer.install(self.home, self.archive, self.sidecar)
        self.assertEqual(self.calls, [])

    def test_symlink_and_modified_unit_rejected(self):
        paths = self.paths()
        installer.safe_directory(paths["bin/omarchy-buzz"].parent)
        paths["bin/omarchy-buzz"].symlink_to(self.archive)
        with self.assertRaisesRegex(ValueError, "unexpected installed path"):
            installer.install(self.home, self.archive, self.sidecar)
        paths["bin/omarchy-buzz"].unlink()
        installer.install(self.home, self.archive, self.sidecar)
        self.calls.clear()
        paths["service/omarchy-buzz.service"].write_text("modified")
        with self.assertRaisesRegex(ValueError, "modified or unrecognized"):
            installer.uninstall(self.home)
        self.assertEqual(self.calls, [])

    def test_failed_activation_restores_previous_files(self):
        installer.install(self.home, self.archive, self.sidecar)
        previous = {name: path.read_bytes() for name, path in self.paths().items()}
        def failure(*args, **kwargs):
            if args[:2] == ("enable", "--now"):
                raise RuntimeError("activation failed")
            return True
        with patch.object(installer, "systemctl", side_effect=failure):
            with self.assertRaisesRegex(RuntimeError, "activation failed"):
                installer.install(self.home, self.archive, self.sidecar)
        self.assertEqual(previous, {name: path.read_bytes() for name, path in self.paths().items()})

    def test_failed_first_activation_removes_staged_files(self):
        calls = []
        def failure(*args, **kwargs):
            calls.append(args)
            if args[:2] == ("enable", "--now"):
                raise RuntimeError("activation failed")
            return True
        with patch.object(installer, "systemctl", side_effect=failure):
            with self.assertRaisesRegex(RuntimeError, "activation failed"):
                installer.install(self.home, self.archive, self.sidecar)
        self.assertTrue(all(not path.exists() for path in self.paths().values()))
        self.assertIn(("disable", "--now", "omarchy-buzz.socket"), calls)

    def test_target_probe_failure_changes_nothing(self):
        self.version_run.return_value = subprocess.CompletedProcess([], 1, b"", b"load failure")
        with self.assertRaisesRegex(ValueError, "target --version probe"):
            installer.install(self.home, self.archive, self.sidecar)
        self.assertEqual(self.calls, [])
        self.assertFalse((self.home / ".local").exists())
        self.assertFalse((self.home / ".config").exists())

    def test_target_probe_mismatched_metadata_rejected(self):
        self.version_run.return_value = subprocess.CompletedProcess(
            [], 0, b'{"helperVersion":"9.9.9","protocolVersion":1,"backendRevision":"781d39510cf23cfe224e8f521ae06a23377e06de"}', b"")
        with self.assertRaisesRegex(ValueError, "differs from package metadata"):
            installer.install(self.home, self.archive, self.sidecar)
        self.assertEqual(self.calls, [])
        self.assertFalse((self.home / ".local").exists())

    def test_failed_upgrade_restores_disabled_socket(self):
        installer.install(self.home, self.archive, self.sidecar)
        state = {"enabled": False, "active": False}
        def partial_failure(*args, **kwargs):
            command = args[0]
            if command == "is-enabled":
                return state["enabled"]
            if command == "is-active":
                return state["active"]
            if command == "enable":
                state["enabled"] = True
                if "--now" in args:
                    state["active"] = True
                    raise RuntimeError("partial activation")
            if command == "disable":
                state["enabled"] = False
                state["active"] = False
            if command == "stop":
                state["active"] = False
            return True
        with patch.object(installer, "systemctl", side_effect=partial_failure):
            with self.assertRaisesRegex(RuntimeError, "partial activation"):
                installer.install(self.home, self.archive, self.sidecar)
        self.assertEqual(state, {"enabled": False, "active": False})
        self.assertTrue(all(path.exists() for path in self.paths().values()))

    def test_systemctl_has_bounded_wait(self):
        self.systemd.stop()
        with patch.object(installer.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")) as run:
            installer.systemctl("daemon-reload")
        self.assertEqual(run.call_args.kwargs["timeout"], 15)

    def rewrite_archive(self, extra=None, replacement=None):
        original = self.archive.read_bytes()
        output = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(original), mode="r:gz") as source:
            with tarfile.open(fileobj=output, mode="w:gz") as target:
                for member in source.getmembers():
                    data = source.extractfile(member).read()
                    if replacement and member.name.endswith("/version.json"):
                        data = replacement
                        member.size = len(data)
                    target.addfile(member, io.BytesIO(data))
                if extra:
                    for member, data in extra:
                        target.addfile(member, io.BytesIO(data) if member.isfile() else None)
        payload = output.getvalue()
        self.archive.write_bytes(payload)
        sidecar = json.loads(self.sidecar.read_text())
        sidecar["artifacts"][0]["sha256"] = hashlib.sha256(payload).hexdigest()
        sidecar["artifacts"][0]["bytes"] = len(payload)
        if replacement:
            sidecar["version"] = json.loads(replacement)
        self.sidecar.write_text(json.dumps(sidecar))

    def test_malformed_documents_and_adversarial_tar_rejected(self):
        original_archive = self.archive.read_bytes()
        original_sidecar = self.sidecar.read_bytes()
        stem = self.archive.name.removesuffix(".tar.gz")
        def member(name, data=b"x", kind=None):
            item = tarfile.TarInfo(stem + "/" + name)
            item.size = len(data)
            if kind is not None:
                item.type = kind
            return item, data
        cases = [
            ("traversal", [member("third-party/../escape")], None),
            ("symlink", [member("third-party/link", b"", tarfile.SYMTYPE)], None),
            ("duplicate", [member("bin/omarchy-buzz")], None),
            ("member budget", [member(f"third-party/{n}") for n in range(1034)], None),
            ("metadata type", None, b"[]"),
        ]
        for label, extra, replacement in cases:
            with self.subTest(label=label):
                self.archive.write_bytes(original_archive)
                self.sidecar.write_bytes(original_sidecar)
                self.rewrite_archive(extra, replacement)
                with self.assertRaises(ValueError):
                    installer.read_package(self.archive, self.sidecar)
        self.archive.write_bytes(original_archive)
        for invalid in (b"[]", b'{"schemaVersion":1,"artifacts":["bad"]}'):
            with self.subTest(sidecar=invalid):
                self.sidecar.write_bytes(invalid)
                with self.assertRaises(ValueError):
                    installer.read_package(self.archive, self.sidecar)


if __name__ == "__main__":
    unittest.main()
