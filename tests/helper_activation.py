#!/usr/bin/env python3
"""Isolated inherited-listener lifecycle test; no identity, relay or systemd."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

DEFAULT_BINARY = "/tmp/omarchy-buzz-build-20260926/debug/omarchy-buzz"
LAUNCHER = """
import os, sys
fd = int(sys.argv[1])
os.dup2(fd, 3, inheritable=True)
if fd != 3:
    os.close(fd)
os.environ['LISTEN_PID'] = str(os.getpid())
os.environ['LISTEN_FDS'] = '1'
os.execv(sys.argv[2], [sys.argv[2], 'daemon'])
"""


def start(binary, listener, env):
    return subprocess.Popen(
        [sys.executable, "-c", LAUNCHER, str(listener.fileno()), binary],
        pass_fds=(listener.fileno(),),
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )


def snapshot(path, child):
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise AssertionError("activated daemon exited before handshake")
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.settimeout(0.4)
                client.connect(str(path))
                with client.makefile("rwb", buffering=0) as stream:
                    hello = json.loads(stream.readline(65537))
                    assert hello["type"] == "hello"
                    client.settimeout(2)
                    stream.write(b'{"version":1,"id":"activation","type":"get_snapshot"}\n')
                    reply = json.loads(stream.readline(65537))
                    assert reply["type"] == "status"
                    assert reply["id"] == "activation"
                    assert reply["instanceId"] == hello["instanceId"]
                    assert reply["status"]["connection"] == "unconfigured"
                    assert reply["status"]["identity"] is None
                    assert reply["status"]["relay"] is None
                    return hello["instanceId"]
        except (TimeoutError, ConnectionRefusedError):
            time.sleep(0.1)
    raise AssertionError("activated daemon handshake deadline exceeded")


def stop(child):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=4)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=2)
    if child.stderr is not None:
        child.stderr.close()


def main():
    binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else DEFAULT_BINARY).resolve())
    assert Path(binary).is_file(), "build helper before running activation test"
    children = []
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="omarchy-buzz-activation-", dir="/tmp") as temp:
        base = Path(temp)
        runtime = base / "runtime"
        config = base / "config"
        home = base / "home"
        for directory in (runtime, config, home, runtime / "omarchy-buzz"):
            directory.mkdir(mode=0o700)
        path = runtime / "omarchy-buzz" / "control.sock"
        # Allowlist the child environment: no inherited identity/provider values.
        env = {"PATH": "/usr/bin:/bin", "HOME": str(home),
               "XDG_RUNTIME_DIR": str(runtime), "XDG_CONFIG_HOME": str(config)}
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
            listener.bind(str(path))
            os.chmod(path, 0o600)
            listener.listen(8)
            original_inode = path.stat().st_ino
            try:
                first = start(binary, listener, env)
                children.append(first)
                first_instance = snapshot(path, first)
                # EOF closes the UI client. Only the daemon's real 30s idle
                # policy is exercised; the parent continues owning its socket.
                assert first.wait(timeout=36) == 0, "idle exit must be successful"
                assert path.exists(), "activated daemon unlinked inherited socket"
                assert path.stat().st_ino == original_inode, "inherited socket was replaced"
                second = start(binary, listener, env)
                children.append(second)
                second_instance = snapshot(path, second)
                assert first_instance != second_instance, "restart reused helper instance identity"
                assert path.stat().st_ino == original_inode
            finally:
                for child in children:
                    stop(child)
    assert time.monotonic() - started < 49, "activation test exceeded its bound"
    print("PASS: inherited socket survives idle exit and serves a new daemon instance")


if __name__ == "__main__":
    main()
