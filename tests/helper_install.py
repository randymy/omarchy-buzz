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


class PackageFixture(unittest.TestCase):
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


class HelperInstall(PackageFixture):
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


class FakeResponse:
    def __init__(self, url, body, length=True, final=None):
        self.url, self.body, self.final, self.status = url, io.BytesIO(body), final or url, 200
        self.headers = {"Content-Length": str(len(body))} if length else {}
        self.reads = 0

    def read(self, size):
        self.reads += 1
        return self.body.read(size)

    def geturl(self):
        return self.final

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


class FakeOpener:
    """Serves release files by name; no network is touched."""
    def __init__(self, files, **options):
        self.files, self.options, self.requests = files, options, []

    def open(self, request, timeout):
        self.requests.append((request.full_url, timeout))
        name = request.full_url.rsplit("/", 1)[1]
        if name not in self.files:
            raise installer.urllib.error.HTTPError(request.full_url, 404, "Not Found", {}, io.BytesIO())
        return FakeResponse(request.full_url, self.files[name], **self.options)


class HelperFetch(PackageFixture):
    def setUp(self):
        super().setUp()
        self.version = json.loads((ROOT / "manifest.json").read_text())["version"]
        self.files = {self.archive.name: self.archive.read_bytes(), self.sidecar.name: self.sidecar.read_bytes()}
        self.cache = self.home / ".cache/omarchy-buzz/helper/releases" / self.version

    def serve(self, files=None, **options):
        opener = FakeOpener(self.files if files is None else files, **options)
        patcher = patch.object(installer.urllib.request, "build_opener", return_value=opener)
        patcher.start()
        self.addCleanup(patcher.stop)
        return opener

    def assert_nothing_installed(self):
        self.assertEqual(self.calls, [])
        self.assertFalse((self.home / ".local").exists())
        self.assertFalse((self.home / ".config").exists())

    def test_fetch_downloads_this_architecture_verifies_and_installs(self):
        opener = self.serve()
        installer.fetch(self.home, self.version)
        base = f"https://github.com/randymy/omarchy-buzz/releases/download/v{self.version}/"
        self.assertEqual([url for url, _ in opener.requests],
                         [base + self.sidecar.name, base + self.archive.name])
        self.assertTrue(all(0 < timeout <= 60 for _, timeout in opener.requests))
        for name in self.files:
            downloaded = self.cache / name
            self.assertEqual(downloaded.read_bytes(), self.files[name])
            self.assertEqual(downloaded.stat().st_mode & 0o777, 0o600)
        self.assertTrue(all(p.is_file() for p in self.paths().values()))
        self.assertIn(("enable", "--now", "omarchy-buzz.socket"), self.calls)

    def test_fetch_checksum_mismatch_installs_nothing(self):
        sidecar = json.loads(self.files[self.sidecar.name])
        sidecar["artifacts"][0]["sha256"] = "0" * 64
        self.files[self.sidecar.name] = json.dumps(sidecar).encode()
        self.serve()
        with self.assertRaisesRegex(ValueError, "checksum"):
            installer.fetch(self.home, self.version)
        self.assert_nothing_installed()
        self.version_run.assert_not_called()

    def test_fetch_oversized_body_rejected_while_reading(self):
        self.serve(length=False)
        with patch.object(installer, "MAX_ARCHIVE", len(self.files[self.archive.name]) - 1):
            with self.assertRaisesRegex(ValueError, "too large"):
                installer.fetch(self.home, self.version)
        self.assertFalse((self.cache / self.archive.name).exists())
        self.assert_nothing_installed()
        self.serve()
        with patch.object(installer, "MAX_SIDECAR", 10):
            with self.assertRaisesRegex(ValueError, "too large"):
                installer.fetch(self.home, self.version)
        self.assert_nothing_installed()

    def test_fetch_wrong_or_unsupported_architecture(self):
        opener = self.serve()
        with patch.object(installer.platform, "machine", return_value="riscv64"):
            with self.assertRaisesRegex(ValueError, "architecture"):
                installer.fetch(self.home, self.version)
        self.assertEqual(opener.requests, [])
        other = "x86_64" if self.archive.name.endswith("aarch64.tar.gz") else "aarch64"
        # The release serves this machine's package under the other
        # architecture's name; the name/sidecar/ELF checks refuse it.
        renamed = {name.replace(self.archive.name.removesuffix(".tar.gz").rsplit("-", 1)[1], other): data
                   for name, data in self.files.items()}
        self.serve(renamed)
        with patch.object(installer.platform, "machine", return_value=other):
            with self.assertRaises(ValueError):
                installer.fetch(self.home, self.version)
        self.assert_nothing_installed()

    def test_fetch_refuses_redirects_off_github(self):
        guard = installer.GitHubRedirects()
        request = installer.urllib.request.Request("https://github.com/randymy/omarchy-buzz/releases/download/v1/x")
        for foreign in ("https://example.com/x", "http://objects.githubusercontent.com/x",
                        "https://objects.githubusercontent.com.evil.example/x", "file:///etc/passwd",
                        "https://user@objects.githubusercontent.com/x"):
            with self.subTest(url=foreign):
                with self.assertRaisesRegex(ValueError, "redirected off GitHub"):
                    guard.redirect_request(request, None, 302, "Found", {}, foreign)
        for allowed in ("https://objects.githubusercontent.com/x", "https://release-assets.githubusercontent.com/x"):
            self.assertIsNotNone(guard.redirect_request(request, None, 302, "Found", {}, allowed))
        # A response that nonetheless ended up elsewhere is refused too.
        self.serve(final="https://example.com/elsewhere")
        with self.assertRaisesRegex(ValueError, "unexpected release download"):
            installer.fetch(self.home, self.version)
        self.assert_nothing_installed()

    def test_fetch_rejects_unsafe_version_and_missing_release(self):
        opener = self.serve({})
        for version in ("../0.0.1", "v0.0.21", "0.0.21/../../x", ""):
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, "release version"):
                    installer.fetch(self.home, version)
        self.assertEqual(opener.requests, [])
        with self.assertRaises(installer.urllib.error.HTTPError) as missing:
            installer.fetch(self.home, self.version)
        missing.exception.close()
        self.assert_nothing_installed()


if __name__ == "__main__":
    unittest.main()
