#!/usr/bin/python3
"""Synthetic stdio helper: no sockets, keys, relay, or network."""
import json
import os
import re
import sys

ROOM = "11111111-1111-4111-8111-111111111111"
INSTANCE = "send-bridge-fixture"
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
assert sys.argv[1:] == ["ui-bridge"]
record = {"sends": [], "fetches": 0, "recipientFetches": 0}
status = {"connection": "authenticated", "identity": "a" * 64,
          "relay": "wss://fixture.invalid", "generation": 7, "category": None,
          "catalog": {"state": "ready", "rooms": [{"id": ROOM, "name": "Synthetic room", "description": ""}], "category": None},
          "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
          "recipients": {"state": "unavailable", "roomId": None, "entries": [], "partial": False, "category": None},
          "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None}}

def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    assert os.stat(os.path.dirname(path)).st_mode & 0o777 == 0o700
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)

def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id,
                      "instanceId": INSTANCE, "generation": 7,
                      "capabilities": ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients"],
                      "status": status}), flush=True)

save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    assert request["version"] == 1
    if request["type"] == "send_message":
        assert UUID.fullmatch(request["id"])
        assert request["roomId"] == ROOM and request["generation"] == 7
        assert request["instanceId"] == INSTANCE and request["mentions"] == ["c" * 64]
        assert request["text"] == ["Synthetic accepted draft", "Synthetic lost draft"][len(record["sends"])]
        record["sends"].append(request)
        save()
        if len(record["sends"]) == 2:
            break  # Deliberately lose the outcome before any delivery receipt.
        status["delivery"] = {"requestId": request["id"], "roomId": ROOM,
                              "eventId": None, "state": "sending", "category": None}
        emit()
        status["delivery"].update(state="acknowledged", eventId="b" * 64)
        emit()
    elif request["type"] == "fetch_recent":
        assert request["roomId"] == ROOM
        record["fetches"] += 1
        save()
        status["history"] = {"state": "snapshot", "roomId": ROOM, "rows": [],
                             "hasMore": False, "category": "history_completeness_unknown"}
        emit(request_id=request["id"])
    elif request["type"] == "fetch_recipients":
        assert request["roomId"] == ROOM
        record["recipientFetches"] += 1
        save()
        status["recipients"] = {"state": "snapshot", "roomId": ROOM,
                                "entries": [{"key": "c" * 64, "name": "Duplicate name"}, {"key": "d" * 64, "name": "Duplicate name"}],
                                "partial": True, "category": None}
        emit(request_id=request["id"])
    elif request["type"] in ("subscribe", "get_snapshot"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request")
