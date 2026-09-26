#!/usr/bin/env python3
"""Actual daemon scope fences; synthetic text, no relay or Secret Service."""
import json
import re
import tomllib
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from helper_smoke import Frames, request, stop


def main():
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="omarchy-buzz-send-ipc-") as temp:
        base = Path(temp)
        env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}
        for variable, name in (("HOME", "home"), ("XDG_RUNTIME_DIR", "runtime"),
                               ("XDG_CONFIG_HOME", "config"), ("XDG_STATE_HOME", "state")):
            (base / name).mkdir(mode=0o700)
            env[variable] = str(base / name)
        version = json.loads(subprocess.check_output([binary, "--version"], env=env, timeout=5))
        root = Path(__file__).resolve().parents[1]
        manifest = tomllib.loads((root / "helper/Cargo.toml").read_text())
        pins = re.findall(r'pub const BUZZ_REVISION:\s*&str\s*=\s*"([0-9a-f]{40})"', (root / "helper/src/compatibility.rs").read_text())
        assert version["protocolVersion"] == 1
        assert version["helperVersion"] == manifest["package"]["version"]
        assert version["helperVersion"] == json.loads((root / "manifest.json").read_text())["version"]
        assert pins == [version["backendRevision"]]
        daemon = subprocess.Popen([binary, "daemon", "--keep-running"], env=env,
                                  stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        sentinel = "synthetic-private-draft-do-not-log"
        try:
            endpoint = base / "runtime/omarchy-buzz/control.sock"
            deadline = time.monotonic() + 5
            while not endpoint.exists():
                assert daemon.poll() is None and time.monotonic() < deadline
                time.sleep(0.02)
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                frames = Frames(client.fileno())
                hello = frames.read()
                client.sendall(request("watch", "subscribe"))
                frames.matching("watch")
                intent = {"version": 1, "id": str(uuid.uuid4()), "type": "send_message",
                          "roomId": str(uuid.uuid4()), "text": sentinel, "mentions": [],
                          "instanceId": hello["instanceId"], "generation": hello["generation"]}
                for field, value in (("instanceId", "stale-instance"), ("generation", hello["generation"] + 1)):
                    stale = dict(intent, **{field: value})
                    client.sendall(json.dumps(stale).encode() + b"\n")
                    reply = frames.matching(intent["id"])
                    assert reply["type"] == "error" and reply["category"] == "send_scope_changed", reply
                    assert sentinel not in json.dumps(reply)
                # Valid local scope still cannot send without authenticated membership.
                client.sendall(json.dumps(intent).encode() + b"\n")
                reply = frames.matching(intent["id"])
                assert reply["type"] == "error" and reply["category"] == "send_unavailable", reply
                assert sentinel not in json.dumps(reply)
                client.sendall(request("offline-status", "get_snapshot"))
                delivery = frames.matching("offline-status")["status"]["delivery"]
                assert delivery["state"] == "idle" and delivery["eventId"] is None, delivery
                assert not list((base / "state").rglob("ledger.json")), "offline send created a durable reservation"
            stop(daemon)
            output, error = daemon.communicate(timeout=5)
            assert sentinel.encode() not in output + error, "draft leaked to diagnostics"
            assert daemon.returncode == 0
            # Version diagnostics work even if the user's config later becomes invalid.
            configuration = base / "config/omarchy-buzz"
            configuration.mkdir(mode=0o700)
            (configuration / "config.toml").write_text("invalid = [")
            assert json.loads(subprocess.check_output([binary, "--version"], env=env, timeout=5)) == version
            print("PASS: actual IPC instance/generation fences, offline send denied, no reservation or text logging")
        finally:
            stop(daemon)
            for stream in (daemon.stdout, daemon.stderr):
                stream.close()


if __name__ == "__main__":
    main()
