#!/usr/bin/env python3
"""Black-box synthetic tests for a staged, built Buzz ACP terminal-auth binary.

Usage: python3 tests/acp_terminal_upstream.py --binary /path/to/buzz-acp
Requires Linux/Unix PTYs. Uses a local fake peer only; no relay or provider.
"""

import argparse
import errno
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest


PEER = Path(__file__).resolve().parent / "upstream-acp-terminal" / "fake-stdio-peer.py"
BINARY = None


def events(trace):
    if not trace.exists():
        return []
    return [json.loads(line) for line in trace.read_text().splitlines() if line]


def names(trace):
    return [row["event"] for row in events(trace)]


class TerminalUpstream(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.TemporaryDirectory(prefix="buzz-acp-terminal-")
        self.addCleanup(self.root.cleanup)
        self.home = Path(self.root.name)
        self.trace = self.home / "trace.jsonl"
        (self.home / "runtime").mkdir()
        self.codex_home = self.home / "codex-profile"
        self.claude_config_dir = self.home / "claude-profile"
        self.codex_home.mkdir()
        self.claude_config_dir.mkdir()
        self.env = {
            "HOME": str(self.home),
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "TERM": "xterm",
            "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_DATA_HOME": str(self.home / "data"),
            "XDG_STATE_HOME": str(self.home / "state"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"),
            "CODEX_HOME": str(self.codex_home),
            "CLAUDE_CONFIG_DIR": str(self.claude_config_dir),
            # Synthetic markers prove the auth subprocesses do not inherit
            # provider or Buzz credentials from the calling environment.
            "BUZZ_PRIVATE_KEY": "synthetic-not-a-key",
            "OPENAI_API_KEY": "synthetic-not-a-key",
            "ANTHROPIC_API_KEY": "synthetic-not-a-key",
            "CODEX_CONFIG": "synthetic-not-a-config",
        }

    def argv(self, subcommand, case, *extra):
        return [
            str(BINARY),
            subcommand,
            "--agent-command", str(PEER),
            "--agent-args", f"{self.trace},{case}",
            *extra,
        ]

    def pipe(self, subcommand, case, *extra):
        return subprocess.run(
            self.argv(subcommand, case, *extra),
            cwd=self.home,
            env=self.env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=8,
            check=False,
        )

    def tty(self, case, *, answer=False, cancel=False):
        argv = self.argv("authenticate", case, "--method-id", "login")
        pid, master = pty.fork()
        if pid == 0:
            os.chdir(self.home)
            os.execve(str(BINARY), argv, self.env)
        os.set_blocking(master, False)
        output = bytearray()
        max_output = 64 * 1024
        sent_answer = False
        sent_cancel = False
        status = None
        reaped = False
        deadline = time.monotonic() + 8
        try:
            while time.monotonic() < deadline:
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    try:
                        chunk = os.read(master, 65536)
                        if chunk:
                            if len(output) + len(chunk) > max_output:
                                self.fail("buzz-acp PTY output exceeded 64 KiB")
                            output.extend(chunk)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                if answer and not sent_answer and b"synthetic-login-input" in output:
                    os.write(master, b"synthetic-answer\n")
                    sent_answer = True
                if cancel and not sent_cancel and "login" in names(self.trace):
                    # Directly signal the harness while its child is foreground.
                    # Its cancellation path must restore the terminal and kill
                    # the login process group.
                    time.sleep(0.1)
                    os.kill(pid, signal.SIGINT)
                    sent_cancel = True
                finished, status = os.waitpid(pid, os.WNOHANG)
                if finished:
                    reaped = True
                    break
            else:
                self.fail(f"buzz-acp timed out; trace={events(self.trace)} output={output[-2000:]!r}")
            # Drain final output after exit; EIO is normal on a closed PTY.
            while True:
                try:
                    chunk = os.read(master, 65536)
                    if not chunk:
                        break
                    if len(output) + len(chunk) > max_output:
                        self.fail("buzz-acp PTY output exceeded 64 KiB")
                    output.extend(chunk)
                except OSError as error:
                    if error.errno in (errno.EIO, errno.EAGAIN):
                        break
                    raise
            return os.waitstatus_to_exitcode(status), output.decode(errors="replace")
        finally:
            if not reaped:
                # Cleanup a failed fixture without leaving a synthetic login
                # process running in its separate foreground process group.
                try:
                    foreground = os.tcgetpgrp(master)
                    if foreground not in (os.getpgrp(), pid):
                        os.killpg(foreground, signal.SIGKILL)
                except (OSError, ProcessLookupError):
                    pass
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    os.waitpid(pid, 0)
                except ChildProcessError:
                    pass
            os.close(master)

    def assert_terminal_success(self, case):
        code, output = self.tty(case, answer=case == "input")
        self.assertEqual(code, 0, output)
        rows = events(self.trace)
        self.assertEqual([r["event"] for r in rows], ["spawn", "initialize", "login", *( ["input"] if case == "input" else [] ), "spawn", "initialize"])
        self.assertEqual([r["terminal"] for r in rows if r["event"] == "initialize"], [True, True])
        login = next(r for r in rows if r["event"] == "login")
        self.assertTrue(login["tty"])
        self.assertTrue(login["foreground"])
        self.assertEqual(login["marker"], "1")
        self.assertNotIn("authenticate", names(self.trace))

    def test_terminal_success_reconnects_without_acp_authenticate(self):
        self.assert_terminal_success("success")

    def test_controlled_profiles_survive_discovery_login_and_reconnect(self):
        result = self.pipe("auth-methods", "success", "--terminal-auth", "--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["methods"][0]["type"], "terminal")
        discovery = events(self.trace)
        self.assertEqual([row["event"] for row in discovery], ["spawn", "initialize"])

        self.trace.unlink()
        self.assert_terminal_success("success")
        rows = discovery + events(self.trace)
        expected = {
            "codex_home": str(self.codex_home),
            "claude_config_dir": str(self.claude_config_dir),
        }
        for row in rows:
            if row["event"] in ("spawn", "login"):
                self.assertEqual({key: row[key] for key in expected}, expected)
        self.assertEqual(len([row for row in rows if row["event"] == "spawn"]), 3)

    def test_terminal_can_read_interactive_input(self):
        self.assert_terminal_success("input")
        self.assertTrue(next(r for r in events(self.trace) if r["event"] == "input")["correct"])

    def test_nonzero_login_does_not_reconnect(self):
        code, output = self.tty("nonzero")
        self.assertNotEqual(code, 0, output)
        self.assertIn("auth_login_failed", output)
        self.assertEqual(names(self.trace), ["spawn", "initialize", "login"])

    def test_cancelled_login_does_not_reconnect(self):
        code, output = self.tty("cancel", cancel=True)
        self.assertNotEqual(code, 0, output)
        self.assertIn("auth_login_cancelled", output)
        self.assertEqual(names(self.trace), ["spawn", "initialize", "login"])

    def test_reinitialize_failure_is_failure_after_successful_login(self):
        code, output = self.tty("reconnect-fail")
        self.assertNotEqual(code, 0, output)
        self.assertIn("auth_reinitialize_failed", output)
        self.assertEqual(names(self.trace), ["spawn", "initialize", "login", "spawn", "initialize"])

    def test_method_descriptor_cannot_choose_command_or_environment(self):
        for case, category in (("command", "auth_command_forbidden"), ("unsafe-env", "auth_env_unsupported"), ("duplicate", "auth_method_ambiguous"), ("unknown-type", "auth_type_unsupported")):
            with self.subTest(case=case):
                self.trace.unlink(missing_ok=True)
                code, output = self.tty(case)
                self.assertNotEqual(code, 0, output)
                self.assertIn(category, output)
                self.assertEqual(names(self.trace), ["spawn", "initialize"])

    def test_absent_agent_type_uses_acp_authenticate(self):
        result = self.pipe("authenticate", "agent", "--method-id", "login")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(names(self.trace), ["spawn", "initialize", "authenticate"])

    def test_no_tty_fails_before_launching_terminal_method(self):
        result = self.pipe("authenticate", "force-terminal", "--method-id", "login")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("auth_terminal_required", result.stderr)
        self.assertEqual(names(self.trace), ["spawn", "initialize"])
        self.assertEqual(events(self.trace)[1]["terminal"], False)

    def test_discovery_advertises_only_executable_terminal_capability(self):
        result = self.pipe("auth-methods", "success", "--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["methods"], [])
        self.assertEqual(events(self.trace)[1]["terminal"], False)
        self.trace.unlink()
        result = self.pipe("auth-methods", "success", "--terminal-auth", "--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["methods"][0]["type"], "terminal")
        self.assertEqual(events(self.trace)[1]["terminal"], True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, help="write sanitized JSON test counts")
    parsed = parser.parse_args()
    BINARY = parsed.binary.resolve(strict=True)
    if not os.access(BINARY, os.X_OK):
        parser.error("--binary must be executable")
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(TerminalUpstream)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    evidence = {
        "testsRun": result.testsRun,
        "failures": len(result.failures),
        "errors": len(result.errors),
        "successful": result.wasSuccessful(),
    }
    if parsed.output:
        parsed.output.write_text(json.dumps(evidence, sort_keys=True) + "\n")
    raise SystemExit(0 if result.wasSuccessful() else 1)
