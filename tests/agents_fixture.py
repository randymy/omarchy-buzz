#!/usr/bin/python3
"""Synthetic stdio helper and agent service: no sockets, keys, units, relay, or network.

`ui-bridge` answers as an authenticated helper with two joined rooms. `agents-bridge`
fakes the agent service of docs/AGENTS_SERVICE.md and records every request it
receives to BUZZ_SEND_RECORD.
"""
import json
import os
import sys

ROOM_A = "11111111-1111-4111-8111-111111111111"
ROOM_B = "22222222-2222-4222-8222-222222222222"
SELF = "a" * 64
AGENT = "33333333-3333-4333-8333-333333333333"
CREATED = "55555555-5555-4555-9555-555555555555"
FIELDS = ["acpCommand", "answersDms", "description", "harness", "instructions", "model", "name", "respondTo",
          "rooms", "startAtLogin", "workspace"]


def helper():
    instance = "agents-helper-fixture"
    rooms = [{"id": room, "name": name, "description": "", "kind": "stream", "participants": [], "hidden": False}
             for room, name in ((ROOM_A, "Synthetic general"), (ROOM_B, "Synthetic builds"))]
    status = {"connection": "authenticated", "identity": SELF, "relay": "wss://fixture.invalid", "generation": 3,
              "category": None, "catalog": {"state": "ready", "rooms": rooms, "category": None},
              "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
              "recipients": {"state": "unavailable", "roomId": None, "entries": [], "partial": False, "category": None}}

    def emit(kind="status", request_id=None):
        print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": instance, "generation": 3,
                          "capabilities": ["connection_status", "room_catalog", "room_history", "room_recipients"],
                          "status": status}), flush=True)

    emit("hello")
    for line in sys.stdin:
        request = json.loads(line)
        kind = request["type"]
        if kind == "fetch_recent":
            status["history"] = {"state": "snapshot", "roomId": request["roomId"], "rows": [], "hasMore": False,
                                 "category": "history_completeness_unknown"}
        elif kind == "fetch_recipients":
            status["recipients"] = {"state": "snapshot", "roomId": request["roomId"],
                                    "entries": [{"key": SELF, "name": "Me"}], "partial": False, "category": None}
        elif kind not in ("subscribe", "get_snapshot"):
            raise AssertionError("Unexpected synthetic helper request")
        emit(request_id=request["id"])


def agents_service():
    instance = "agents-service-fixture"
    record = {"requests": []}
    harnesses = [{"id": "claude-code", "bundle": "ready", "signedIn": False},
                 {"id": "codex", "bundle": "ready", "signedIn": True}]
    agents = [{"id": AGENT, "name": "Fixture agent", "description": "Synthetic persona", "instructions": "Answer briefly.",
               "harness": "codex", "model": "", "acpCommand": "buzz-acp", "rooms": [ROOM_A], "respondTo": "owner-only",
               "workspace": "/home/fixture/.local/state/omarchy-buzz-room-workspaces/" + AGENT, "identity": "b" * 64,
               "enrolled": True, "unit": "inactive", "startAtLogin": False, "answersDms": False, "published": True, "lastError": None}]
    state = {"pending": None}

    def save():
        path = os.environ["BUZZ_SEND_RECORD"]
        assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
        with open(path + ".tmp", "w", encoding="utf-8") as output:
            json.dump(record, output)
        os.replace(path + ".tmp", path)

    def emit(kind="status", request_id=None):
        print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": instance,
                          "capabilities": ["agent_manager"],
                          "status": {"harnesses": harnesses, "agents": agents, "pending": state["pending"]}}), flush=True)

    def working(request):
        assert state["pending"] is None or state["pending"]["state"] != "working", "second mutating request"
        state["pending"] = {"requestId": request["id"], "type": request["type"], "state": "working", "category": None}
        emit(request_id=request["id"])

    def done(request):
        state["pending"] = {"requestId": request["id"], "type": request["type"], "state": "done", "category": None}
        emit(request_id=request["id"])

    save()
    emit("hello")
    for line in sys.stdin:
        request = json.loads(line)
        assert request["version"] == 1 and request["instanceId"] == instance
        kind = request["type"]
        record["requests"].append(request)
        save()
        if kind == "subscribe":
            assert sorted(request) == ["id", "instanceId", "type", "version"]
            emit(request_id=request["id"])
        elif kind == "create_agent":
            assert sorted(request) == ["fields", "id", "instanceId", "type", "version"]
            fields = request["fields"]
            assert sorted(fields) == FIELDS and fields["acpCommand"] == "buzz-acp"
            if fields["name"] == "Rejected agent":
                # Refused before any state changed: an error frame and nothing pending.
                print(json.dumps({"version": 1, "type": "error", "id": request["id"], "instanceId": instance,
                                  "category": "agent_invalid"}), flush=True)
                continue
            working(request)
            agents.append({"id": CREATED, "name": fields["name"], "description": fields["description"],
                           "instructions": fields["instructions"], "harness": fields["harness"], "model": fields["model"],
                           "acpCommand": "buzz-acp", "rooms": fields["rooms"], "respondTo": fields["respondTo"],
                           "workspace": fields["workspace"] or "/home/fixture/.local/state/omarchy-buzz-room-workspaces/" + CREATED,
                           "identity": None, "enrolled": False, "unit": "inactive",
                           "startAtLogin": fields["startAtLogin"], "answersDms": fields["answersDms"],
                           "published": False, "lastError": None})
            done(request)
        elif kind == "update_agent":
            assert sorted(request) == ["agentId", "fields", "id", "instanceId", "type", "version"] and request["agentId"] == AGENT
            assert request["fields"] == {"answersDms": True}, request
            working(request)
            agents[0]["answersDms"] = True
            done(request)
        elif kind == "start_agent":
            assert sorted(request) == ["agentId", "id", "instanceId", "type", "version"] and request["agentId"] == AGENT
            working(request)
            agents[0]["unit"] = "active"
            done(request)
        elif kind == "stop_agent":
            # A malformed answer: an undocumented unit state must end the panel's session.
            agents[0]["unit"] = "running"
            emit(request_id=request["id"])
        else:
            raise AssertionError("Unexpected synthetic agent request")


if sys.argv[1:] == ["ui-bridge"]:
    helper()
elif sys.argv[1:] == ["agents-bridge"]:
    agents_service()
else:
    raise AssertionError("Unexpected synthetic helper arguments")
