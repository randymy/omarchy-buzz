#!/usr/bin/python3
"""Synthetic stdio helper for starting a direct message: no sockets, keys, relay, or network."""
import json
import os
import sys

ROOM = "11111111-1111-4111-8111-111111111111"
DM = "44444444-4444-4444-8444-444444444444"
SELF = "a" * 64
MEMBER = "c" * 64
INSTANCE = "new-dm-fixture"
STREAM = {"id": ROOM, "name": "Synthetic room", "description": "", "kind": "stream", "participants": [], "hidden": False}
OPENED = {"id": DM, "name": "Synthetic person", "description": "", "kind": "dm", "participants": [SELF, MEMBER], "hidden": False}
ROSTER = [{"key": SELF, "name": "Me"}, {"key": MEMBER, "name": "Synthetic person"}]
IDLE = {"state": "idle", "requestId": None, "channelId": None, "created": None, "category": None}
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
status = {"connection": "authenticated", "identity": SELF,
          "relay": "wss://fixture.invalid", "generation": 7, "category": None,
          "catalog": {"state": "ready", "rooms": [STREAM], "category": None},
          "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
          "recipients": {"state": "unavailable", "roomId": None, "entries": [], "partial": False, "category": None},
          "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None},
          "dmOpen": dict(IDLE)}

def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)

def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id,
                      "instanceId": INSTANCE, "generation": 7,
                      "capabilities": ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients", "dm_open"],
                      "status": status}), flush=True)

def note(entry):
    record["requests"].append(entry)
    save()

def listed(room):
    return any(entry["id"] == room for entry in status["catalog"]["rooms"])

save()
emit("hello")
while True:
    line = sys.stdin.readline()
    if not line:
        break
    request = json.loads(line)
    assert request["version"] == 1
    kind = request["type"]
    if kind == "open_dm":
        # Only the verified member was chosen; never the viewer, never a raw event.
        assert sorted(request) == ["generation", "id", "instanceId", "participants", "type", "version"]
        assert request["participants"] == [MEMBER]
        assert request["generation"] == 7 and request["instanceId"] == INSTANCE
        assert status["dmOpen"]["state"] != "sending"
        note("open_dm")
        status["dmOpen"] = {"state": "sending", "requestId": request["id"], "channelId": None, "created": None, "category": None}
        emit(request_id=request["id"])
        # The relay's OK names the channel before the joined-room check lists it.
        status["dmOpen"] = {"state": "acknowledged", "requestId": request["id"], "channelId": DM, "created": True, "category": None}
        emit()
        status["catalog"] = {"state": "ready", "rooms": [STREAM, OPENED], "category": None}
        note("room_check_listed_dm")
        emit()
    elif kind == "fetch_recent":
        assert listed(request["roomId"])
        note("fetch_recent " + request["roomId"][:8])
        status["history"] = {"state": "snapshot", "roomId": request["roomId"], "rows": [],
                             "hasMore": False, "category": "history_completeness_unknown"}
        emit(request_id=request["id"])
    elif kind == "fetch_recipients":
        assert listed(request["roomId"])
        note("fetch_recipients " + request["roomId"][:8])
        status["recipients"] = {"state": "snapshot", "roomId": request["roomId"], "entries": ROSTER,
                                "partial": False, "category": None}
        emit(request_id=request["id"])
    elif kind in ("subscribe", "get_snapshot"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request")
