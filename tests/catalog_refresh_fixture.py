#!/usr/bin/python3
"""Synthetic stdio helper replaying the periodic joined-room check: no sockets, keys, relay, or network."""
import json
import os
import select
import sys

ROOM = "11111111-1111-4111-8111-111111111111"
INSTANCE = "catalog-refresh-fixture"
ROOMS = [{"id": ROOM, "name": "Synthetic room", "description": ""}]
NO_HISTORY = {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None}
NO_RECIPIENTS = {"state": "unavailable", "roomId": None, "entries": [], "partial": False, "category": None}
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
status = {"connection": "authenticated", "identity": "a" * 64,
          "relay": "wss://fixture.invalid", "generation": 7, "category": None,
          "catalog": {"state": "ready", "rooms": ROOMS, "category": None},
          "history": dict(NO_HISTORY), "recipients": dict(NO_RECIPIENTS),
          "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None}}

def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)

def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id,
                      "instanceId": INSTANCE, "generation": 7,
                      "capabilities": ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients"],
                      "status": status}), flush=True)

def note(entry):
    record["requests"].append(entry)
    save()

save()
emit("hello")
checked = False
while True:
    line = sys.stdin.readline()
    if not line:
        break
    request = json.loads(line)
    assert request["version"] == 1
    kind = request["type"]
    if kind == "send_message":
        assert request["roomId"] == ROOM and request["text"] == "Synthetic held draft"
        assert request["mentions"] == ["c" * 64]
        # The real helper refuses sends until its roster and history are re-established.
        assert status["history"]["state"] == "snapshot" and status["recipients"]["state"] == "snapshot"
        note("send_message")
        status["delivery"] = {"requestId": request["id"], "roomId": ROOM, "eventId": "b" * 64,
                              "state": "acknowledged", "category": None}
        emit()
    elif kind == "fetch_recent":
        assert request["roomId"] == ROOM and status["catalog"]["state"] == "ready"
        note("fetch_recent")
        status["history"] = {"state": "snapshot", "roomId": ROOM, "rows": [],
                             "hasMore": False, "category": "history_completeness_unknown"}
        emit(request_id=request["id"])
    elif kind == "fetch_recipients":
        assert request["roomId"] == ROOM and status["catalog"]["state"] == "ready"
        note("fetch_recipients")
        status["recipients"] = {"state": "snapshot", "roomId": ROOM,
                                "entries": [{"key": "c" * 64, "name": "Synthetic person"}],
                                "partial": False, "category": None}
        emit(request_id=request["id"])
        if not checked:
            checked = True
            status["catalog"] = {"state": "loading", "rooms": [], "category": None}
            status["history"] = dict(NO_HISTORY)
            status["recipients"] = dict(NO_RECIPIENTS)
            note("room_check_started")
            emit()
            # Nothing may be requested or sent while joined rooms are unconfirmed.
            if select.select([sys.stdin], [], [], 1.0)[0]:
                note("request_during_room_check")
            status["catalog"] = {"state": "ready", "rooms": ROOMS, "category": None}
            note("room_check_finished")
            emit()
    elif kind in ("subscribe", "get_snapshot"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request")
