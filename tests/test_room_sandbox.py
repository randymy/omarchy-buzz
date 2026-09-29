"""Synthetic filesystem and environment checks for the room-agent launcher."""
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/room-sandbox"
loader = importlib.machinery.SourceFileLoader("room_sandbox", str(SCRIPT))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)


class RoomSandbox(unittest.TestCase):
    def test_private_mounts_and_clean_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, workspace, bundle = (root / name for name in ("profile", "work", "bundle"))
            for path in (profile, workspace, bundle, *(profile / n for n in module.PROFILE_DIRS)):
                path.mkdir(mode=0o700)
            (root / "outside").write_text("host sentinel")
            (workspace / "writable").write_text("before")
            module.validate(profile, workspace, bundle)
            probe = '''import json, os
from pathlib import Path
Path("/workspace/writable").write_text("after")
try:
    Path("/usr/room-sandbox-write-probe").write_text("bad")
    runtime_readonly = False
except OSError:
    runtime_readonly = True
print(json.dumps({"outside": Path("%s").exists(), "host_home": Path(os.environ["HOME"]).as_posix(), "env": dict(os.environ), "workspace": Path("/workspace/writable").read_text(), "runtime_readonly": runtime_readonly}))''' % (root / "outside")
            command = module.build_command(profile, workspace, bundle,
                                           ["/usr/bin/python3", "-c", probe],
                                           "ws://127.0.0.1:7777")
            ambient = os.environ | {"OPENAI_API_KEY": "synthetic", "DISPLAY": ":999",
                                    "DBUS_SESSION_BUS_ADDRESS": "unix:path=/tmp/synthetic-bus"}
            result = subprocess.run(command, text=True, capture_output=True, timeout=15, env=ambient)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertFalse(report["outside"])
            self.assertEqual(report["workspace"], "after")
            self.assertTrue(report["runtime_readonly"])
            self.assertEqual((workspace / "writable").read_text(), "after")
            self.assertEqual(report["host_home"], "/profile/home")
            self.assertEqual(report["env"]["CODEX_HOME"], "/profile/provider")
            self.assertEqual(report["env"]["BUZZ_RELAY_URL"], "ws://127.0.0.1:7777")
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "OPENAI_API_KEY"):
                self.assertNotIn(key, report["env"])

    def test_key_option_uses_memory_fd_not_process_arguments(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, work, bundle = [root / n for n in ("profile", "work", "bundle")]
            for path in (profile, work, bundle, *(profile / n for n in module.PROFILE_DIRS)):
                path.mkdir(mode=0o700)
            command = module.build_command(profile, work, bundle,
                ["/usr/bin/python3", "-c", "import os; assert os.environ['BUZZ_PRIVATE_KEY'] == 'synthetic-test-identity'"])
            fd = os.memfd_create("synthetic-options", 0)
            try:
                os.write(fd, b"--setenv\0BUZZ_PRIVATE_KEY\0synthetic-test-identity\0")
                os.lseek(fd, 0, 0)
                index = command.index("--chdir")
                command[index:index] = ["--args", str(fd)]
                self.assertNotIn("synthetic-test-identity", command)
                result = subprocess.run(command, pass_fds=(fd,), capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
            finally:
                os.close(fd)

    def test_rejects_linked_or_overlapping_mounts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, bundle = root / "profile", root / "bundle"
            profile.mkdir(mode=0o700)
            bundle.mkdir(mode=0o700)
            for name in module.PROFILE_DIRS:
                (profile / name).mkdir(mode=0o700)
            with self.assertRaisesRegex(module.Refused, "workspace_profile_overlap"):
                module.validate(profile, profile / "home", bundle)
            linked = root / "linked"
            linked.symlink_to(profile, target_is_directory=True)
            with self.assertRaisesRegex(module.Refused, "linked_directory_path"):
                module.validate(linked, profile / "home", bundle)
            with self.assertRaisesRegex(module.Refused, "relay_origin_required"):
                module.relay_url("ws://user:secret@127.0.0.1:7777")


if __name__ == "__main__":
    unittest.main()
