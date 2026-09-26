#!/usr/bin/env python3
"""Isolated, unconfigured process smoke check; requires an already built helper."""

import json
import os
from pathlib import Path
import selectors
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time


class Frames:
    def __init__(self, fd):
        self.fd = fd
        self.buffer = b""

    def read(self):
        deadline = time.monotonic() + 5
        with selectors.DefaultSelector() as poll:
            poll.register(self.fd, selectors.EVENT_READ)
            while b"\n" not in self.buffer:
                remaining = deadline - time.monotonic()
                assert remaining > 0 and poll.select(remaining), "response timed out"
                data = os.read(self.fd, 8192)
                assert data, "unexpected response EOF"
                self.buffer += data
                assert len(self.buffer) <= 65536, "oversized response"
        line, self.buffer = self.buffer.split(b"\n", 1)
        return json.loads(line)

    def matching(self, request_id):
        # A subscription may emit an initial unsolicited status update.
        for _ in range(5):
            frame = self.read()
            if frame.get("id") == request_id:
                return frame
        raise AssertionError("missing correlated response")


def status(frame, kind):
    assert frame["version"] == 1 and frame["type"] == kind, frame
    assert frame["status"]["connection"] == "unconfigured", frame
    assert frame["status"]["identity"] is None, frame
    assert frame["status"]["relay"] is None, frame
    assert frame["capabilities"] == ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients"], frame
    assert frame["status"]["catalog"]["state"] == "unavailable", frame
    assert frame["status"]["catalog"]["rooms"] == [], frame
    assert frame["status"]["history"] == {
        "state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None,
    }, frame
    assert frame["status"]["delivery"] == {
        "state": "idle", "requestId": None, "roomId": None, "eventId": None, "category": None,
    }, frame
    assert frame["status"]["recipients"] == {
        "state": "unavailable", "roomId": None, "entries": [], "partial": True, "category": None,
    }, frame
    assert frame["instanceId"] and frame["generation"] == 1, frame


def request(request_id, kind):
    return json.dumps({"version": 1, "id": request_id, "type": kind}).encode() + b"\n"


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python3 tests/helper_smoke.py /absolute/path/to/omarchy-buzz")
    binary = Path(sys.argv[1]).resolve(strict=True)
    processes = []
    with tempfile.TemporaryDirectory(prefix="omarchy-buzz-smoke-") as temp:
        base = Path(temp)
        for name in ("home", "config", "runtime", "state"):
            (base / name).mkdir(mode=0o700)
        # Do not inherit keyring/session bus, credential, logging, or activation env.
        env = {
            "HOME": str(base / "home"),
            "XDG_CONFIG_HOME": str(base / "config"),
            "XDG_STATE_HOME": str(base / "state"),
            "XDG_RUNTIME_DIR": str(base / "runtime"),
            "LANG": "C.UTF-8",
        }
        endpoint = base / "runtime/omarchy-buzz/control.sock"
        try:
            daemon = subprocess.Popen(
                [str(binary), "daemon", "--keep-running"], env=env,
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            processes.append(daemon)
            deadline = time.monotonic() + 5
            while not endpoint.exists():
                assert daemon.poll() is None, "daemon exited before socket appeared"
                assert time.monotonic() < deadline, "daemon socket timed out"
                time.sleep(0.02)
            assert stat.S_IMODE(endpoint.stat().st_mode) == 0o600
            assert stat.S_IMODE(endpoint.parent.stat().st_mode) == 0o700

            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                frames = Frames(client.fileno())
                hello = frames.read()
                status(hello, "hello")
                for request_id, kind in (("subscribe1", "subscribe"), ("snapshot1", "get_snapshot")):
                    client.sendall(request(request_id, kind))
                    reply = frames.matching(request_id)
                    status(reply, "status")
                    assert reply["instanceId"] == hello["instanceId"]
                client.sendall(b"not-json\n")
                for _ in range(5):
                    error = frames.read()
                    if error.get("type") == "error":
                        break
                assert error["type"] == "error" and error["category"] == "invalid_request", error
                assert client.recv(1) == b"", "malformed client was not closed"

            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                status(Frames(client.fileno()).read(), "hello")
                client.sendall(b"x" * 65537 + b"\n")
                try:
                    closed = client.recv(1) == b""
                except ConnectionResetError:
                    closed = True
                assert closed, "oversized daemon request was not rejected"

            bridge = subprocess.Popen(
                [str(binary), "ui-bridge"], env=env, bufsize=0,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            processes.append(bridge)
            frames = Frames(bridge.stdout.fileno())
            status(frames.read(), "hello")
            bridge.stdin.write(request("bridge1", "get_snapshot"))
            status(frames.matching("bridge1"), "status")
            bridge.stdin.close()
            assert bridge.wait(timeout=5) == 0, "bridge failed to exit cleanly on stdin EOF"

            for payload, category in ((b"invalid\n", b"invalid_request"), (b"x" * 65537 + b"\n", b"oversized_request")):
                invalid_bridge = subprocess.Popen(
                    [str(binary), "ui-bridge"], env=env,
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                processes.append(invalid_bridge)
                _, error = invalid_bridge.communicate(payload, timeout=5)
                assert invalid_bridge.returncode != 0 and error.strip() == category, error

            assert daemon.poll() is None, "bad clients killed daemon"
            daemon.send_signal(signal.SIGTERM)
            assert daemon.wait(timeout=5) == 0, "SIGTERM did not stop daemon cleanly"
            assert not endpoint.exists(), "standalone daemon left its socket behind"
            assert not list((base / "config").rglob("*")), "unconfigured run wrote configuration"
            print("PASS: unconfigured hello/status, malformed and oversized requests, bridge EOF, SIGTERM socket cleanup")
        finally:
            for process in reversed(processes):
                stop(process)
                for pipe in (process.stdin, process.stdout, process.stderr):
                    if pipe is not None:
                        pipe.close()


if __name__ == "__main__":
    main()
