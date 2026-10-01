#!/usr/bin/env python3
"""Isolated agent-service process smoke check; requires an already built helper.

The daemon runs with OMARCHY_BUZZ_AGENTS_FAKE_CONTROL=1: unit control, the
keyring, the script spawner and the room source are in-memory fakes, so no
systemd unit, Secret Service item, script or relay is touched. HOME, the XDG
directories and the runtime directory are private temporary directories.
"""

import hashlib
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

ROOM = "00000000-0000-4000-8000-0000000000b1"
# Public key of the synthetic secret 1 (NIP-OA test vector); never a real identity.
OWNER = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
AGENT_KEYS = ["id", "name", "description", "instructions", "harness", "model", "acpCommand", "rooms",
              "respondTo", "workspace", "identity", "enrolled", "unit", "startAtLogin", "answersDms", "published", "lastError",
              "relay", "community", "instances"]
INSTANCE_KEYS = ["relay", "community", "rooms", "unit", "startAtLogin", "published", "lastError", "state"]
RELAY = "ws://127.0.0.1:9/"
OTHER_RELAY = "ws://127.0.0.1:7/"
# Public key of the synthetic secret 2 (NIP-OA test vector): an enrolled agent's identity.
AGENT_IDENTITY = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5"


def unit_name(agent_id, relay=None):
    """The unit of an agent's primary instance, or of its instance in `relay`."""
    if relay is None:
        return "omarchy-buzz-agent-%s.service" % agent_id
    return "omarchy-buzz-agent-%s-%s.service" % (agent_id, hashlib.sha256(relay.encode()).hexdigest()[:12])


def v1_persona(agent):
    """A version 2 agent's first instance as a version 1 persona record."""
    first = dict(agent["instances"][0])
    del first["primary"]
    persona = {k: v for k, v in agent.items() if k != "instances"}
    persona.update(first)
    return persona


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
                data = os.read(self.fd, 65536)
                assert data, "unexpected response EOF"
                self.buffer += data
                assert len(self.buffer) <= 1048576, "oversized response"
        line, self.buffer = self.buffer.split(b"\n", 1)
        return json.loads(line)

    def answer(self, request_id):
        # Unsolicited status updates may precede the correlated answer.
        for _ in range(20):
            frame = self.read()
            if frame.get("id") == request_id:
                return frame
        raise AssertionError("missing correlated response")


def check_status(frame, kind, instance=None):
    assert set(frame) == {"version", "type", "id", "instanceId", "capabilities", "status"}, frame
    assert frame["version"] == 1 and frame["type"] == kind, frame
    assert frame["capabilities"] == ["agent_manager"], frame
    if instance is not None:
        assert frame["instanceId"] == instance, frame
    status = frame["status"]
    assert set(status) == {"activeRelay", "harnesses", "agents", "pending", "modelProbe"}, status
    assert status["activeRelay"] == RELAY, status
    assert status["modelProbe"] == {"agentId": None, "state": "idle", "model": "", "detail": None}, status
    assert [h["id"] for h in status["harnesses"]] == ["claude-code", "codex"], status
    for harness in status["harnesses"]:
        # Fake mode has no reviewed scripts: nothing is reported ready.
        assert harness == {"id": harness["id"], "bundle": "missing", "signedIn": None}, harness
    for agent in status["agents"]:
        assert sorted(agent) == sorted(AGENT_KEYS), agent
        assert 1 <= len(agent["instances"]) <= 4, agent
        for instance in agent["instances"]:
            assert sorted(instance) == sorted(INSTANCE_KEYS), instance
        # The top level repeats the first instance.
        first = agent["instances"][0]
        assert (agent["relay"], agent["community"], agent["rooms"], agent["startAtLogin"], agent["published"],
                agent["lastError"], agent["unit"]) == (first["relay"], first["community"], first["rooms"],
                                                      first["startAtLogin"], first["published"], first["lastError"],
                                                      first["state"]), agent
    return status


def request(request_id, instance, kind, **fields):
    value = {"version": 1, "id": request_id, "instanceId": instance, "type": kind, **fields}
    return json.dumps(value).encode() + b"\n"


def uuid(n):
    return "00000000-0000-4000-8000-%012d" % n


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
        raise SystemExit("Usage: python3 tests/agents_smoke.py /absolute/path/to/omarchy-buzz")
    binary = Path(sys.argv[1]).resolve(strict=True)
    processes = []
    with tempfile.TemporaryDirectory(prefix="omarchy-buzz-agents-smoke-") as temp:
        base = Path(temp)
        for name in ("home", "config", "runtime", "state", "data"):
            (base / name).mkdir(mode=0o700)
        (base / "config/omarchy-buzz").mkdir(mode=0o700)
        config = base / "config/omarchy-buzz/config.toml"
        # Format 2 (a community list); a format-1 file would be migrated and
        # leave a config.v1.toml backup beside it, which the write check below
        # would then have to expect.
        config.write_text('version = 2\nidentity = "%s"\nactiveRelay = "ws://127.0.0.1:9/"\n\n'
                          '[[communities]]\nrelay = "ws://127.0.0.1:9/"\nname = "Smoke"\njoinedAt = 1\n' % OWNER)
        config.chmod(0o600)
        # Do not inherit keyring/session bus, credential, logging, or activation env.
        env = {
            "HOME": str(base / "home"),
            "XDG_CONFIG_HOME": str(base / "config"),
            "XDG_STATE_HOME": str(base / "state"),
            "XDG_DATA_HOME": str(base / "data"),
            "XDG_RUNTIME_DIR": str(base / "runtime"),
            "LANG": "C.UTF-8",
            "OMARCHY_BUZZ_AGENTS_FAKE_CONTROL": "1",
            "OMARCHY_BUZZ_AGENTS_FAKE_ROOMS": ROOM,
        }
        endpoint = base / "runtime/omarchy-buzz/agents.sock"
        store = base / "state/omarchy-buzz/agents/personas.json"
        try:
            daemon = subprocess.Popen(
                [str(binary), "agents-daemon", "--keep-running"], env=env,
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
                check_status(hello, "hello")
                assert hello["id"] is None and hello["status"]["agents"] == [] and hello["status"]["pending"] is None
                instance = hello["instanceId"]

                client.sendall(request(uuid(1), instance, "subscribe"))
                check_status(frames.answer(uuid(1)), "status", instance)

                fields = {"name": "Smoke", "description": "Synthetic.", "instructions": "Say hi.",
                          "harness": "codex", "model": "", "rooms": [ROOM], "respondTo": "owner-only",
                          "workspace": "", "startAtLogin": False, "acpCommand": "buzz-acp", "answersDms": False}
                client.sendall(request(uuid(2), instance, "create_agent", fields=fields))
                status = check_status(frames.answer(uuid(2)), "status", instance)
                assert status["pending"] == {"requestId": uuid(2), "type": "create_agent", "state": "done", "category": None,
                                             "detail": None}, status
                [agent] = status["agents"]
                assert agent["name"] == "Smoke" and agent["enrolled"] is False and agent["identity"] is None
                assert agent["unit"] == "inactive" and agent["published"] is False and agent["lastError"] is None
                # Bound to the active community, named from the configuration.
                assert agent["relay"] == RELAY and agent["community"] == "Smoke", agent
                assert agent["instances"] == [{"relay": RELAY, "community": "Smoke", "rooms": [ROOM],
                                               "unit": unit_name(agent["id"]), "startAtLogin": False,
                                               "published": False, "lastError": None, "state": "inactive"}], agent
                saved = json.loads(store.read_text())
                assert saved["version"] == 2, saved
                record = saved["agents"][0]
                assert record["instances"][0]["relay"] == RELAY and record["instances"][0]["primary"] is True, record
                workspace = Path(agent["workspace"])
                assert workspace == base / "state/omarchy-buzz-room-workspaces" / agent["id"], agent
                assert stat.S_IMODE(workspace.stat().st_mode) == 0o700
                assert stat.S_IMODE(store.stat().st_mode) == 0o600

                client.sendall(request(uuid(3), instance, "update_agent", agentId=agent["id"], fields={"name": "Renamed"}))
                status = check_status(frames.answer(uuid(3)), "status", instance)
                assert status["pending"]["state"] == "done" and status["agents"][0]["name"] == "Renamed", status
                assert json.loads(store.read_text())["agents"][0]["name"] == "Renamed"
                assert json.loads(store.read_text())["agents"][0]["instances"][0]["relay"] == RELAY
                record = json.loads(store.read_text())["agents"][0]

                # Refused requests: a bad field, an unknown room, an unknown harness.
                for n, kind, extra in (
                    (4, "update_agent", {"agentId": agent["id"], "fields": {"name": ""}}),
                    (5, "update_agent", {"agentId": agent["id"], "fields": {"command": "/bin/sh"}}),
                    (6, "create_agent", {"fields": {**fields, "rooms": [uuid(99)]}}),
                    (7, "sign_in", {"harness": "bash"}),
                ):
                    client.sendall(request(uuid(n), instance, kind, **extra))
                    frame = frames.answer(uuid(n))
                    if frame["type"] == "error":
                        assert frame == {"version": 1, "type": "error", "id": uuid(n), "instanceId": instance,
                                         "category": "agent_invalid"}, frame
                    else:
                        assert frame["status"]["pending"] == {"requestId": uuid(n), "type": kind, "state": "failed",
                                                              "category": "agent_invalid", "detail": None}, frame
                # sign_in for a known harness: the reviewed script is absent.
                client.sendall(request(uuid(8), instance, "sign_in", harness="codex"))
                assert frames.answer(uuid(8))["status"]["pending"]["category"] == "harness_missing"
                # A model of the other harness: refused with the detail, nothing saved.
                client.sendall(request(uuid(12), instance, "update_agent", agentId=agent["id"], fields={"model": "opus"}))
                status = check_status(frames.answer(uuid(12)), "status", instance)
                assert status["pending"] == {"requestId": uuid(12), "type": "update_agent", "state": "failed",
                                             "category": "agent_invalid", "detail": "model_not_for_harness"}, status
                assert status["agents"][0]["model"] == "" and json.loads(store.read_text())["agents"][0]["model"] == ""
                client.sendall(request(uuid(13), instance, "update_agent", agentId=agent["id"], fields={"model": "gpt-5.5"}))
                assert check_status(frames.answer(uuid(13)), "status", instance)["agents"][0]["model"] == "gpt-5.5"
                # probe_model: no reviewed scripts in fake mode, so nothing runs.
                client.sendall(request(uuid(14), instance, "probe_model", agentId=agent["id"]))
                status = check_status(frames.answer(uuid(14)), "status", instance)
                assert status["pending"]["category"] == "harness_missing", status
                client.sendall(request(uuid(15), instance, "probe_model", agentId=agent["id"], model="opus"))
                assert frames.answer(uuid(15)) == {"version": 1, "type": "error", "id": uuid(15), "instanceId": instance,
                                                   "category": "agent_invalid"}
                # Instances: a never-enrolled agent cannot join another community,
                # the only community cannot be left, a relay it is not in is refused.
                for n, kind, extra in (
                    (16, "enroll_agent_in", {"agentId": agent["id"], "relay": RELAY, "rooms": [ROOM]}),
                    (17, "leave_agent_community", {"agentId": agent["id"], "relay": RELAY}),
                    (18, "start_agent", {"agentId": agent["id"], "relay": OTHER_RELAY}),
                ):
                    client.sendall(request(uuid(n), instance, kind, **extra))
                    status = check_status(frames.answer(uuid(n)), "status", instance)
                    assert status["pending"] == {"requestId": uuid(n), "type": kind, "state": "failed",
                                                 "category": "agent_invalid", "detail": None}, status
                # Malformed: nine rooms, no rooms key, a non-canonical relay.
                for n, extra in (
                    (19, {"agentId": agent["id"], "relay": RELAY, "rooms": [uuid(k) for k in range(9)]}),
                    (20, {"agentId": agent["id"], "relay": RELAY}),
                    (21, {"agentId": agent["id"], "relay": "ws://127.0.0.1:9", "rooms": [ROOM]}),
                ):
                    client.sendall(request(uuid(n), instance, "enroll_agent_in", **extra))
                    assert frames.answer(uuid(n)) == {"version": 1, "type": "error", "id": uuid(n),
                                                      "instanceId": instance, "category": "agent_invalid"}
                assert len(json.loads(store.read_text())["agents"][0]["instances"]) == 1
                # refresh_bundle: the installed agent-bundle is absent.
                client.sendall(request(uuid(11), instance, "refresh_bundle", harness="codex"))
                assert frames.answer(uuid(11))["status"]["pending"]["category"] == "harness_missing"

                client.sendall(request(uuid(9), instance, "delete_agent", agentId=agent["id"]))
                status = check_status(frames.answer(uuid(9)), "status", instance)
                assert status["agents"] == [] and status["pending"]["state"] == "done", status
                assert json.loads(store.read_text())["agents"] == []
                # The user's default workspace is kept.
                assert workspace.is_dir()

                client.sendall(b"not-json\n")
                deadline = time.monotonic() + 5
                while True:
                    try:
                        data = client.recv(65536)
                    except ConnectionResetError:
                        data = b""
                    if not data:
                        break
                    assert b'"type":"error"' not in data
                    assert time.monotonic() < deadline, "malformed client was not closed"

            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                check_status(Frames(client.fileno()).read(), "hello")
                client.sendall(b"x" * 65537 + b"\n")
                try:
                    closed = client.recv(1) == b""
                except ConnectionResetError:
                    closed = True
                assert closed, "oversized request was not rejected"

            bridge = subprocess.Popen(
                [str(binary), "agents-bridge"], env=env, bufsize=0,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            processes.append(bridge)
            frames = Frames(bridge.stdout.fileno())
            hello = frames.read()
            check_status(hello, "hello")
            bridge.stdin.write(request(uuid(10), hello["instanceId"], "subscribe"))
            check_status(frames.answer(uuid(10)), "status")
            bridge.stdin.close()
            assert bridge.wait(timeout=5) == 0, "bridge failed to exit cleanly on stdin EOF"

            invalid = subprocess.Popen(
                [str(binary), "agents-bridge"], env=env,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            processes.append(invalid)
            _, error = invalid.communicate(b"invalid\n", timeout=5)
            assert invalid.returncode != 0 and error.strip() == b"invalid_request", error

            assert daemon.poll() is None, "bad clients killed daemon"
            daemon.send_signal(signal.SIGTERM)
            assert daemon.wait(timeout=5) == 0, "SIGTERM did not stop daemon cleanly"
            assert not endpoint.exists(), "standalone daemon left its socket behind"
            written = sorted(str(p.relative_to(base)) for p in (base / "config").rglob("*"))
            assert written == ["config/omarchy-buzz", "config/omarchy-buzz/config.toml"], written

            # A store written before personas had a relay: one persona whose
            # generated unit names another relay keeps that one, the other
            # takes the configuration's first community. Migrated once on load.
            older = []
            for n in (1, 2):
                persona = dict(v1_persona(record), id="00000000-0000-4000-8000-0000000000a%d" % n, name="Old %d" % n,
                               workspace=str(base / ("home/old%d" % n)))
                del persona["relay"]
                older.append(persona)
            store.write_text(json.dumps({"version": 1, "agents": older}))
            store.chmod(0o600)
            units = base / "config/systemd/user"
            units.mkdir(parents=True, mode=0o700)
            unit = units / ("omarchy-buzz-agent-%s.service" % older[0]["id"])
            unit.write_text('[Service]\nExecStart="/x/room-agent" "--harness" "codex" "--relay" "ws://127.0.0.1:7/" '
                            '"--room" "%s"\n' % ROOM)
            unit.chmod(0o600)
            unit_text = unit.read_text()
            daemon = subprocess.Popen(
                [str(binary), "agents-daemon", "--keep-running"], env=env,
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            processes.append(daemon)
            deadline = time.monotonic() + 5
            while not endpoint.exists():
                assert daemon.poll() is None, "daemon refused the older store"
                assert time.monotonic() < deadline, "daemon socket timed out"
                time.sleep(0.02)
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                agents = check_status(Frames(client.fileno()).read(), "hello")["agents"]
                assert [(a["name"], a["relay"], a["community"]) for a in agents] == [
                    ("Old 1", "ws://127.0.0.1:7/", "127.0.0.1:7"), ("Old 2", RELAY, "Smoke")], agents
                # Each wrapped persona is its agent's primary instance: same unit name.
                assert [[i["unit"] for i in a["instances"]] for a in agents] == [
                    [unit_name(older[0]["id"])], [unit_name(older[1]["id"])]], agents
            migrated = json.loads(store.read_text())
            assert migrated["version"] == 2, migrated
            assert [[(i["relay"], i["primary"], i["workspace"]) for i in a["instances"]] for a in migrated["agents"]] == [
                [("ws://127.0.0.1:7/", True, older[0]["workspace"])], [(RELAY, True, older[1]["workspace"])]], migrated
            backup = store.parent / "personas.v1.json"
            assert json.loads(backup.read_text()) == {"version": 1, "agents": older}
            assert stat.S_IMODE(store.stat().st_mode) == 0o600 and stat.S_IMODE(backup.stat().st_mode) == 0o600
            assert unit.read_text() == unit_text, "the unit file was changed"
            daemon.send_signal(signal.SIGTERM)
            assert daemon.wait(timeout=5) == 0

            # A version 2 store with an agent enrolled in two communities: both
            # instances reported with their own units; one community is left
            # (no memberships recorded, so nothing is published), the last is not.
            two = dict(migrated["agents"][1], identity=AGENT_IDENTITY)
            second = dict(two["instances"][0], relay=OTHER_RELAY, primary=False, workspace=str(base / "home/second"))
            two["instances"] = [two["instances"][0], second]
            store.write_text(json.dumps({"version": 2, "agents": [two]}))
            daemon = subprocess.Popen(
                [str(binary), "agents-daemon", "--keep-running"], env=env,
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            processes.append(daemon)
            deadline = time.monotonic() + 5
            while not endpoint.exists():
                assert daemon.poll() is None, "daemon refused the version 2 store"
                assert time.monotonic() < deadline, "daemon socket timed out"
                time.sleep(0.02)
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(str(endpoint))
                frames = Frames(client.fileno())
                hello = frames.read()
                [agent] = check_status(hello, "hello")["agents"]
                instance = hello["instanceId"]
                assert agent["enrolled"] is True and agent["relay"] == RELAY, agent
                assert [(i["relay"], i["community"], i["unit"]) for i in agent["instances"]] == [
                    (RELAY, "Smoke", unit_name(two["id"])),
                    (OTHER_RELAY, "127.0.0.1:7", unit_name(two["id"], OTHER_RELAY))], agent
                client.sendall(request(uuid(30), instance, "leave_agent_community", agentId=two["id"], relay=OTHER_RELAY))
                status = check_status(frames.answer(uuid(30)), "status", instance)
                assert status["pending"]["state"] == "done", status
                assert [i["relay"] for i in status["agents"][0]["instances"]] == [RELAY], status
                client.sendall(request(uuid(31), instance, "leave_agent_community", agentId=two["id"], relay=RELAY))
                status = check_status(frames.answer(uuid(31)), "status", instance)
                assert status["pending"]["category"] == "agent_invalid", status
            assert [i["relay"] for i in json.loads(store.read_text())["agents"][0]["instances"]] == [RELAY]
            daemon.send_signal(signal.SIGTERM)
            assert daemon.wait(timeout=5) == 0
            assert not list((base / "data").rglob("*")), "fake mode wrote into the data directory"
            print("PASS: agents hello/status, create/update/delete round trip with the agent's community, "
                  "older store migrated from the unit file and the first community and wrapped into version 2 "
                  "with its unit names, instances reported and left, refused instance requests, refused requests, "
                  "harness model check and probe gating, "
                  "malformed and oversized frames, bridge EOF, SIGTERM socket cleanup")
        finally:
            for process in reversed(processes):
                stop(process)
                for pipe in (process.stdin, process.stdout, process.stderr):
                    if pipe is not None:
                        pipe.close()


if __name__ == "__main__":
    main()
