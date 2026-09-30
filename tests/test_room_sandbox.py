"""Synthetic filesystem and environment checks for the room-agent launcher."""
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/room-sandbox"
loader = importlib.machinery.SourceFileLoader("room_sandbox", str(SCRIPT))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)

AGENT = Path(__file__).resolve().parents[1] / "scripts/room-agent"
agent_loader = importlib.machinery.SourceFileLoader("room_agent", str(AGENT))
agent_spec = importlib.util.spec_from_loader(agent_loader.name, agent_loader)
room_agent = importlib.util.module_from_spec(agent_spec)
agent_loader.exec_module(room_agent)
module = room_agent.sandbox  # one Refused class for both scripts

OWNER = "a" * 64
IDENTITY = "b" * 64
ROOM = "0b5ad4b1-3f55-4a55-9f0a-2d6a1c6c1a01"
ROOM2 = "5c1f0f0e-8d8c-4c4e-8f1e-7b0e0e0e0e02"
# The Codex agent command exactly as the September 29 launcher built it.
LEGACY_CODEX = ['/opt/agent/bin/room-agent-entry', '--agent-command', 'codex-acp',
                '--agent-owner', OWNER, '--channels', ROOM, '--subscribe', 'mentions',
                '--respond-to', 'owner-only', '--permission-mode', 'default', '--agents', '1',
                '--heartbeat-interval', '0', '--max-turn-duration', '180', '--idle-timeout', '60']


def make_tree(root, harness="codex"):
    profile, workspace, bundle, private = (root / n for n in ("profile", "work", "bundle", "private"))
    for path in (profile, workspace, bundle, private, *(profile / n for n in module.PROFILE_DIRS)):
        path.mkdir(mode=0o700)
    (bundle / "bin").mkdir()
    (bundle / "bin" / room_agent.AGENT_COMMANDS[harness]).write_text("#!/bin/sh\n")
    instructions = private / "instructions"
    instructions.write_text("Be brief.\n")
    instructions.chmod(0o600)
    return profile, workspace, bundle, instructions


def agent_args(root, *extra, harness=None):
    profile, workspace, bundle, _ = (root / n for n in ("profile", "work", "bundle", "private"))
    argv = ["--profile", str(profile), "--workspace", str(workspace), "--bundle", str(bundle),
            "--relay", "wss://relay.example", "--room", ROOM, "--owner", OWNER, "--identity", IDENTITY]
    if harness:
        argv += ["--harness", harness]
    parser = room_agent.parser()
    args = parser.parse_args(argv + list(extra))
    room_agent.check_arguments(parser, args)
    return args


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



class RoomAgentLauncher(unittest.TestCase):
    def test_codex_defaults_render_the_legacy_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, workspace, bundle, _ = make_tree(root)
            args = agent_args(root)
            self.assertEqual(room_agent.agent_command(args), LEGACY_CODEX)
            expected = module.build_command(profile, workspace, bundle, LEGACY_CODEX, "wss://relay.example")
            index = expected.index("--chdir")
            expected[index:index] = ["--args", "3"]
            self.assertEqual(room_agent.launch_argv(args), expected)

    def test_codex_golden_with_new_flags(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, _, _, instructions = make_tree(root)
            args = agent_args(root, "--room", ROOM2, "--respond-to", "mentions",
                              "--instructions", str(instructions), "--model", "gpt-5.5")
            command = room_agent.agent_command(args)
            self.assertEqual(command, ['/opt/agent/bin/room-agent-entry', '--agent-command', 'codex-acp',
                '--agent-owner', OWNER, '--channels', ROOM + ',' + ROOM2, '--subscribe', 'mentions',
                '--respond-to', 'anyone', '--permission-mode', 'default', '--agents', '1',
                '--heartbeat-interval', '0', '--max-turn-duration', '180', '--idle-timeout', '60',
                '--system-prompt-file', '/run/agent/instructions', '--model', 'gpt-5.5'])
            argv = room_agent.launch_argv(args)
            self.assertEqual(argv[argv.index("--") + 1:], command)
            bind = argv.index(str(instructions))
            self.assertEqual(argv[bind - 1:bind + 2], ["--ro-bind", str(instructions), "/run/agent/instructions"])
            self.assertLess(argv.index("/run"), bind)
            self.assertNotIn("ANTHROPIC_MODEL", argv)
            self.assertNotIn("CLAUDE_CONFIG_DIR", argv)

    def test_answers_dms_golden_omits_channels(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, workspace, bundle, _ = make_tree(root)
            parser = room_agent.parser()
            args = parser.parse_args(["--profile", str(profile), "--workspace", str(workspace), "--bundle", str(bundle),
                                      "--relay", "wss://relay.example", "--answers-dms", "--owner", OWNER,
                                      "--identity", IDENTITY])
            room_agent.check_arguments(parser, args)
            expected = ['/opt/agent/bin/room-agent-entry', '--agent-command', 'codex-acp',
                        '--agent-owner', OWNER, '--subscribe', 'mentions',
                        '--respond-to', 'owner-only', '--permission-mode', 'default', '--agents', '1',
                        '--heartbeat-interval', '0', '--max-turn-duration', '180', '--idle-timeout', '60']
            self.assertEqual(room_agent.agent_command(args), expected)
            argv = room_agent.launch_argv(args)
            self.assertEqual(argv[argv.index("--") + 1:], expected)
            # Rooms given anyway are validated but never become a channel filter.
            with_rooms = agent_args(root, "--room", ROOM2, "--answers-dms")
            self.assertEqual(room_agent.agent_command(with_rooms), expected)
            self.assertNotIn("--channels", room_agent.launch_argv(with_rooms))

    def test_claude_code_golden(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, _, _, instructions = make_tree(root, "claude-code")
            args = agent_args(root, "--instructions", str(instructions), "--model", "claude-sonnet-4-5",
                              harness="claude-code")
            command = room_agent.agent_command(args)
            self.assertEqual(command, ['/opt/agent/bin/room-agent-entry', '--agent-command', 'claude-agent-acp',
                '--agent-owner', OWNER, '--channels', ROOM, '--subscribe', 'mentions',
                '--respond-to', 'owner-only', '--permission-mode', 'default', '--agents', '1',
                '--heartbeat-interval', '0', '--max-turn-duration', '180', '--idle-timeout', '60',
                '--system-prompt-file', '/run/agent/instructions'])
            argv = room_agent.launch_argv(args)
            self.assertEqual(argv[argv.index("--") + 1:], command)
            env = {argv[i + 1]: argv[i + 2] for i, v in enumerate(argv) if v == "--setenv"}
            self.assertEqual(env["CLAUDE_CONFIG_DIR"], "/profile/provider")
            self.assertEqual(env["DISABLE_AUTOUPDATER"], "1")
            self.assertEqual(env["ANTHROPIC_MODEL"], "claude-sonnet-4-5")
            self.assertEqual(env["HOME"], "/profile/home")
            self.assertEqual(argv[argv.index("--args"):argv.index("--args") + 4],
                             ["--args", "3", "--chdir", "/workspace"])
            for name in ("ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN", "BUZZ_PRIVATE_KEY"):
                self.assertNotIn(name, env)

    def test_claude_settings_that_reroute_are_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, _, _, _ = make_tree(root, "claude-code")
            args = agent_args(root, harness="claude-code")
            settings = profile / "provider/settings.json"
            settings.write_text('{"forceLoginMethod": "claudeai", "hooks": {}}')
            room_agent.launch_argv(args)
            for text in ('{"apiKeyHelper": "echo key"}', '{"env": {"ANTHROPIC_BASE_URL": "x"}}',
                         '{"forceLoginMethod": "console"}', '[]', 'not json'):
                settings.write_text(text)
                with self.assertRaisesRegex(module.Refused, "provider_settings_review_required"):
                    room_agent.launch_argv(args)

    def test_bundle_must_match_harness(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_tree(root, "codex")
            with self.assertRaisesRegex(module.Refused, "bundle_harness_mismatch"):
                room_agent.launch_argv(agent_args(root, harness="claude-code"))

    def test_argument_validation_categories(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_tree(root)
            base = ["--profile", str(root / "profile"), "--workspace", str(root / "work"),
                    "--bundle", str(root / "bundle"), "--relay", "wss://relay.example",
                    "--owner", OWNER, "--identity", IDENTITY]
            rooms = [f"00000000-0000-4000-8000-00000000000{i}" for i in range(9)]
            cases = {
                "room_count_invalid": sum((["--room", r] for r in rooms), []),
                # Without --answers-dms a room is required.
                "room_count_invalid ": [],
                "room_count_invalid  ": ["--answers-dms", *sum((["--room", r] for r in rooms), [])],
                "duplicate_room ": ["--answers-dms", "--room", ROOM, "--room", ROOM],
                "duplicate_room": ["--room", ROOM, "--room", ROOM],
                "canonical_room_required": ["--room", ROOM.upper()],
                "model_invalid": ["--room", ROOM, "--model", "gpt 5; rm"],
                "invalid choice": ["--room", ROOM, "--respond-to", "anyone"],
            }
            cases["invalid choice "] = ["--room", ROOM, "--harness", "goose"]
            for category, extra in cases.items():
                # Every case fails before the Secret Service lookup.
                result = subprocess.run([sys.executable, str(AGENT), *base, *extra],
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 2, category)
                self.assertIn(category.strip(), result.stderr)
            result = subprocess.run([sys.executable, str(AGENT), *base[:-2], "--identity", OWNER, "--room", ROOM],
                                    capture_output=True, text=True, timeout=10)
            self.assertIn("separate_agent_and_owner_required", result.stderr)

    def test_instructions_file_is_validated(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, workspace, bundle, instructions = make_tree(root)
            check = lambda path: module.instructions_file(path, profile, workspace, bundle)
            self.assertEqual(check(instructions), instructions)
            for inside in (workspace / "i", profile / "provider/i"):
                inside.write_text("x"); inside.chmod(0o600)
                with self.assertRaisesRegex(module.Refused, "instructions_path_overlap"):
                    check(inside)
            instructions.chmod(0o644)
            with self.assertRaisesRegex(module.Refused, "instructions_file_permissions_unsafe"):
                check(instructions)
            instructions.chmod(0o600)
            (root / "private").chmod(0o755)
            with self.assertRaisesRegex(module.Refused, "private_directory_permissions_unsafe"):
                check(instructions)
            (root / "private").chmod(0o700)
            link = root / "private/link"
            link.symlink_to(instructions)
            with self.assertRaisesRegex(module.Refused, "linked_instructions_path"):
                check(link)
            with self.assertRaisesRegex(module.Refused, "absolute_instructions_file_required"):
                check(Path("relative"))
            instructions.write_bytes(b"x" * (16 * 1024 + 1))
            with self.assertRaisesRegex(module.Refused, "instructions_file_too_large"):
                check(instructions)
            instructions.write_bytes(b"\xff\xfe")
            with self.assertRaisesRegex(module.Refused, "instructions_file_invalid"):
                check(instructions)
            with self.assertRaisesRegex(module.Refused, "instructions_file_missing"):
                check(root / "private/absent")

    def test_instructions_are_read_only_inside_the_sandbox(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, workspace, bundle, instructions = make_tree(root)
            probe = """from pathlib import Path
p = Path('/run/agent/instructions')
text = p.read_text()
try:
    p.write_text('changed'); writable = True
except OSError:
    writable = False
print(text.strip(), writable)"""
            command = module.build_command(profile, workspace, bundle, ["/usr/bin/python3", "-c", probe],
                                           harness="claude-code", instructions=instructions,
                                           extra_env={"ANTHROPIC_MODEL": "m"})
            result = subprocess.run(command, capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "Be brief. False")
            self.assertEqual(instructions.read_text(), "Be brief.\n")
            with self.assertRaisesRegex(module.Refused, "sandbox_environment_refused"):
                module.build_command(profile, workspace, bundle, ["/usr/bin/true"],
                                     extra_env={"ANTHROPIC_API_KEY": "x"})
            with self.assertRaisesRegex(module.Refused, "harness_unknown"):
                module.build_command(profile, workspace, bundle, ["/usr/bin/true"], harness="goose")


# A structurally valid synthetic attestation (the launcher does not verify the
# signature; buzz-acp and the service do).
SIGNATURE = "c" * 128
TAG = ["auth", OWNER, "", SIGNATURE]
TAG_JSON = '["auth","%s","","%s"]' % (OWNER, SIGNATURE)


def write_tag(root, value, name="auth-tag.json"):
    path = root / "private" / name
    path.write_text(value if isinstance(value, str) else json.dumps(value, indent=1))
    path.chmod(0o600)
    return path


class RoomAgentAuthTag(unittest.TestCase):
    def test_auth_tag_file_is_validated(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile, workspace, bundle, _ = make_tree(root)
            tag = write_tag(root, TAG)
            check = lambda path: room_agent.auth_tag_value(agent_args(root, "--auth-tag", str(path)))
            # Re-encoded canonically, as buzz-sdk compute_auth_tag emits it.
            self.assertEqual(check(tag), TAG_JSON.encode())
            conditioned = write_tag(root, ["auth", OWNER, "kind=1&created_at<1713957000", SIGNATURE], "c.json")
            self.assertEqual(json.loads(check(conditioned))[2], "kind=1&created_at<1713957000")
            self.assertIsNone(room_agent.auth_tag_value(agent_args(root)))
            for inside in (workspace / "t", profile / "provider/t", bundle / "t"):
                inside.write_text(TAG_JSON); inside.chmod(0o600)
                with self.assertRaisesRegex(module.Refused, "auth_tag_path_overlap"):
                    check(inside)
            tag.chmod(0o640)
            with self.assertRaisesRegex(module.Refused, "auth_tag_file_permissions_unsafe"):
                check(tag)
            tag.chmod(0o600)
            (root / "private").chmod(0o750)
            with self.assertRaisesRegex(module.Refused, "private_directory_permissions_unsafe"):
                check(tag)
            (root / "private").chmod(0o700)
            link = root / "private/link"
            link.symlink_to(tag)
            with self.assertRaisesRegex(module.Refused, "linked_auth_tag_path"):
                check(link)
            with self.assertRaisesRegex(module.Refused, "absolute_auth_tag_file_required"):
                check(Path("relative/auth-tag.json"))
            with self.assertRaisesRegex(module.Refused, "auth_tag_file_missing"):
                check(root / "private/absent")
            directory = root / "private/dir"
            directory.mkdir(mode=0o700)
            with self.assertRaisesRegex(module.Refused, "auth_tag_file_permissions_unsafe"):
                check(directory)
            big = write_tag(root, " " * (4 * 1024 - len(TAG_JSON)) + TAG_JSON, "big.json")
            self.assertEqual(check(big), TAG_JSON.encode())
            big.write_text(" " * (4 * 1024 + 1 - len(TAG_JSON)) + TAG_JSON)
            with self.assertRaisesRegex(module.Refused, "auth_tag_file_too_large"):
                check(big)
            invalid = [
                "not json", "\xff", '{"auth": 1}', "[]", json.dumps(TAG[:3]), json.dumps(TAG + [""]),
                json.dumps(["AUTH", *TAG[1:]]), json.dumps(["auth", OWNER.upper(), "", SIGNATURE]),
                json.dumps(["auth", OWNER[:-1], "", SIGNATURE]), json.dumps(["auth", OWNER, "", SIGNATURE[:-2]]),
                json.dumps(["auth", OWNER, "", "g" * 128]), json.dumps(["auth", OWNER, 1, SIGNATURE]),
                json.dumps(["auth", OWNER, "kind=01", SIGNATURE]), json.dumps(["auth", OWNER, "kind=65536", SIGNATURE]),
                json.dumps(["auth", OWNER, "kind=1&", SIGNATURE]), json.dumps(["auth", OWNER, "color=red", SIGNATURE]),
                json.dumps(["auth", OWNER, "kind=1 ", SIGNATURE]),
            ]
            for index, text in enumerate(invalid):
                path = root / "private" / ("invalid-%d.json" % index)
                path.write_bytes(text.encode("utf-8", "surrogateescape") if text != "\xff" else b"\xff")
                path.chmod(0o600)
                with self.assertRaisesRegex(module.Refused, "auth_tag_file_invalid", msg=text):
                    check(path)
            other = write_tag(root, ["auth", "d" * 64, "", SIGNATURE], "other.json")
            with self.assertRaisesRegex(module.Refused, "auth_tag_owner_mismatch"):
                check(other)

    def test_refusals_happen_before_the_secret_service_lookup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_tree(root)
            other = write_tag(root, ["auth", "d" * 64, "", SIGNATURE], "other.json")
            base = ["--profile", str(root / "profile"), "--workspace", str(root / "work"),
                    "--bundle", str(root / "bundle"), "--relay", "wss://relay.example",
                    "--room", ROOM, "--owner", OWNER, "--identity", IDENTITY]
            for path, category in ((other, "auth_tag_owner_mismatch"),
                                   (root / "private/absent", "auth_tag_file_missing")):
                # No session bus: even a missed check could not reach Secret Service.
                result = subprocess.run([sys.executable, str(AGENT), *base, "--auth-tag", str(path)],
                                        capture_output=True, text=True, timeout=10,
                                        env={"PATH": "/usr/bin", "HOME": str(root),
                                             "DBUS_SESSION_BUS_ADDRESS": "unix:path=/nonexistent"})
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(category, result.stderr)
                self.assertNotIn(SIGNATURE, result.stderr)

    def test_memfd_options_golden(self):
        key = b"1" * 64
        self.assertEqual(room_agent.identity_options(key, None),
                         b"--setenv\0BUZZ_PRIVATE_KEY\0" + key + b"\0")
        self.assertEqual(room_agent.identity_options(key, TAG_JSON.encode()),
                         b"--setenv\0BUZZ_PRIVATE_KEY\0" + key + b"\0"
                         b"--setenv\0BUZZ_AUTH_TAG\0" + TAG_JSON.encode() + b"\0")

    def test_auth_tag_reaches_the_sandbox_only_through_the_memfd(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_tree(root)
            tag = write_tag(root, TAG)
            args = agent_args(root, "--auth-tag", str(tag))
            argv = room_agent.launch_argv(args)
            value = room_agent.auth_tag_value(args)
            # Neither the file nor its contents are in the process arguments or mounts.
            self.assertFalse(any(str(tag) in word or SIGNATURE in word for word in argv))
            self.assertEqual(argv[argv.index("--args") + 1], "3")
            probe = ("import json, os; print(json.dumps([os.environ.get('BUZZ_AUTH_TAG'), "
                     "os.environ.get('BUZZ_PRIVATE_KEY'), os.path.exists('/run/agent')]))")
            command = argv[:argv.index("--") + 1] + ["/usr/bin/python3", "-c", probe]
            command[command.index("--args") + 1] = "{fd}"
            fd = os.memfd_create("synthetic-options", 0)
            try:
                os.write(fd, room_agent.identity_options(b"2" * 64, value))
                os.lseek(fd, 0, 0)
                command[command.index("{fd}")] = str(fd)
                result = subprocess.run(command, pass_fds=(fd,), capture_output=True, text=True, timeout=15)
            finally:
                os.close(fd)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), [TAG_JSON, "2" * 64, False])


if __name__ == "__main__":
    unittest.main()
