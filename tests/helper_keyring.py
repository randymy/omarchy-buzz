#!/usr/bin/env python3
"""Optional real Secret Service smoke; synthetic identity, private D-Bus and HOME.

Usage: python3 tests/helper_keyring.py /absolute/path/to/omarchy-buzz
Missing system dependencies produce an explicit SKIP. No live relay is used.
"""
import errno
import fcntl
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import pty
import selectors
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import termios
import threading
import time

from helper_smoke import Frames, request, stop

# Scalar 1 is public test material, never a production identity.
SECRET = b"0" * 63 + b"1"
PUBLIC = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
RELAY_SECRET = b"0" * 63 + b"2"
RELAY_PUBLIC = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5"


def clean_output(data):
    assert SECRET not in data, "synthetic private key leaked into process output"
    assert RELAY_SECRET not in data, "synthetic relay key leaked into process output"


def run(binary, *args):
    result = subprocess.run([binary, *args], capture_output=True, timeout=10)
    clean_output(result.stdout + result.stderr)
    assert result.returncode == 0, "helper command failed: " + " ".join(args)
    return result.stdout


def enroll(binary, secret=SECRET, expected_error=None):
    master, slave = pty.openpty()
    process = None
    output = bytearray()
    try:
        def terminal():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        process = subprocess.Popen(
            [binary, "setup", "identity", "enroll"], stdin=slave, stdout=slave,
            stderr=slave, preexec_fn=terminal,
        )
        os.close(slave)
        slave = None
        sent = False
        deadline = time.monotonic() + 20
        with selectors.DefaultSelector() as poll:
            poll.register(master, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                assert remaining > 0, "identity enrollment timed out"
                if not sent and b"Private key (hidden): " in output:
                    if not termios.tcgetattr(master)[3] & termios.ECHO:
                        os.write(master, secret + b"\n")
                        sent = True
                if not poll.select(min(remaining, 0.2)):
                    if process.poll() is not None:
                        break
                    continue
                try:
                    data = os.read(master, 8192)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                if not data:
                    break
                output.extend(data)
                assert len(output) < 65536, "oversized enrollment output"
                if not sent and b"Private key (hidden): " in output:
                    # Wait for the terminal's echo disable, not merely the prompt.
                    if not termios.tcgetattr(master)[3] & termios.ECHO:
                        os.write(master, secret + b"\n")
                        sent = True
            assert sent, "hidden-input prompt was not reached"
        status = process.wait(timeout=5)
        clean_output(output)
        if expected_error:
            assert status != 0 and expected_error.encode() in output, output
        else:
            assert status == 0, "identity enrollment failed: " + output.decode(errors="replace")
            assert PUBLIC.encode() in output and b'"enrolled"' in output
    finally:
        if process is not None:
            stop(process)
        os.close(master)
        if slave is not None:
            os.close(slave)


def session(binary):
    assert os.environ.get("OMARCHY_BUZZ_TEST_SESSION") == "isolated"
    assert os.environ.get("DBUS_SESSION_BUS_ADDRESS"), "private bus missing"
    keyring = subprocess.Popen(
        [shutil.which("gnome-keyring-daemon"), "--foreground", "--unlock", "--components=secrets"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    daemon = None
    try:
        keyring.stdin.write(b"synthetic-test-keyring-password\n")
        keyring.stdin.close()
        keyring.stdin = None
        class NIP11(BaseHTTPRequestHandler):
            def do_GET(self):
                assert self.path == "/info", self.path
                body = json.dumps({"self": RELAY_PUBLIC}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/nostr+json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args):
                pass

        with HTTPServer(("127.0.0.1", 0), NIP11) as relay:
            relay_thread = threading.Thread(target=relay.serve_forever, daemon=True)
            relay_thread.start()
            origin = "ws://127.0.0.1:%d/" % relay.server_port
            run(binary, "setup", "relay", origin)
            # Enrollment exercises Secret Service activation on this private bus.
            enroll(binary)
            # A later relay-key entry must leave the selected human unchanged.
            enroll(binary, RELAY_SECRET, "identity_is_relay_signer")
            inspected = json.loads(run(binary, "inspect"))
            assert inspected["identity"] == PUBLIC and inspected["relay"] == origin
            assert inspected["configured"] is True
            config = Path(os.environ["XDG_CONFIG_HOME"]) / "omarchy-buzz/config.toml"
            contents = config.read_bytes()
            clean_output(contents)
            assert PUBLIC.encode() in contents and origin.encode() in contents
            assert config.stat().st_mode & 0o777 == 0o600
            relay.shutdown()
            relay_thread.join(timeout=5)
            relay.server_close()
            daemon = subprocess.Popen(
                [binary, "daemon", "--keep-running"], stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            endpoint = Path(os.environ["XDG_RUNTIME_DIR"]) / "omarchy-buzz/control.sock"
            deadline = time.monotonic() + 20
            while not endpoint.exists():
                assert daemon.poll() is None, "daemon exited before socket"
                assert time.monotonic() < deadline, "socket timed out"
                time.sleep(0.02)
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                frames = Frames(client.fileno())
                frame = frames.read()
                while True:
                    clean_output(json.dumps(frame).encode())
                    state = frame["status"]
                    assert state["identity"] == PUBLIC and state["relay"] == origin
                    if state["connection"] == "disconnected":
                        assert state["category"] == "relay_unavailable", frame
                        break
                    assert state["connection"] == "connecting" or (
                        state["connection"] == "unconfigured" and state["category"] is None
                    ), frame
                    assert time.monotonic() < deadline, "identity retrieval timed out"
                    client.sendall(request("probe", "get_snapshot"))
                    frame = frames.matching("probe")
                    time.sleep(0.05)
            daemon.send_signal(signal.SIGTERM)
            assert daemon.wait(timeout=5) == 0
            assert not endpoint.exists(), "daemon left socket"
        print("PASS: NIP-11 guarded enrollment, relay-key collision preserves human identity, private Secret Service, daemon key retrieval, no key output")
    finally:
        for process in (daemon, keyring):
            if process is not None:
                stop(process)
                stdout, stderr = process.communicate(timeout=5)
                clean_output(stdout + stderr)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--isolated-session":
        session(sys.argv[2])
        return
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python3 tests/helper_keyring.py /absolute/path/to/omarchy-buzz")
    missing = [name for name in ("dbus-run-session", "gnome-keyring-daemon") if not shutil.which(name)]
    if missing:
        print("SKIP: missing " + ", ".join(missing))
        return
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="omarchy-buzz-keyring-") as temp:
        base = Path(temp)
        env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "OMARCHY_BUZZ_TEST_SESSION": "isolated"}
        for variable, name in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"),
                               ("XDG_DATA_HOME", "data"), ("XDG_RUNTIME_DIR", "runtime"),
                               ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state")):
            path = base / name
            path.mkdir(mode=0o700)
            env[variable] = str(path)
        process = subprocess.Popen(
            [shutil.which("dbus-run-session"), "--", sys.executable, str(Path(__file__).resolve()),
             "--isolated-session", binary], env=env, start_new_session=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        try:
            stdout, stderr = process.communicate(timeout=50)
            clean_output(stdout + stderr)
            sys.stdout.buffer.write(stdout)
            if process.returncode:
                sys.stderr.buffer.write(stderr)
                raise SystemExit(process.returncode)
        finally:
            # End every private-session descendant, including D-Bus activations.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            if process.poll() is None:
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=3)


if __name__ == "__main__":
    main()
