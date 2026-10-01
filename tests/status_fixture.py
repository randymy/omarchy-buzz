#!/usr/bin/python3
"""Synthetic stdio helper for "Update your status": no sockets, keys, relay, or network.

It answers like the helper: a room whose roster carries another member's
verified status, and this identity's status view. The first `set_status` is
refused by the relay (`status_rejected` in the view), the second is refused by
the helper's rate limit (an error frame, nothing signed), the third is
accepted; `clear_status` is accepted. Every request is recorded.
"""
import json
import os
import re
import sys
import time

INSTANCE = "status-fixture"
IDENTITY = "5" * 64
OTHER = "8" * 64
ROOM = "00000000-0000-4000-8000-0000000000b1"
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "room_history", "room_recipients", "user_status"]
assert sys.argv[1:] == ["ui-bridge"]
here = os.path.dirname(os.path.abspath(__file__))
record = {"requests": []}
ROWS = [
    {"id": "a" * 64, "author": OTHER, "time": 1790000000, "text": "Back on Monday", "edited": False,
     "truncated": False, "unavailable": False},
    {"id": "b" * 64, "author": IDENTITY, "time": 1790000060, "text": "Feel better", "edited": False,
     "truncated": False, "unavailable": False},
]
ENTRIES = [
    {"key": IDENTITY, "name": "Fixture Me", "status": None},
    {"key": OTHER, "name": "Robin", "status": {"text": "Out sick", "emoji": "🤒"}},
]
status = {
    "connection": "authenticated", "identity": IDENTITY, "relay": "wss://fixture.example/", "generation": 1, "category": None,
    "catalog": {"state": "ready", "rooms": [{"id": ROOM, "name": "team", "description": "", "kind": "stream",
                                              "participants": [], "hidden": False}], "category": None},
    "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
    "recipients": {"state": "unavailable", "roomId": None, "entries": [], "agents": [], "partial": True, "category": None},
    "userStatus": {"state": "ready", "mine": None, "category": None},
}


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(here, "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE,
                      "generation": status["generation"], "capabilities": CAPABILITIES, "status": status}), flush=True)


def refuse(request, category):
    print(json.dumps({"version": 1, "type": "error", "id": request["id"], "instanceId": INSTANCE,
                      "category": category}), flush=True)


def mine_in_roster(mine):
    status["recipients"]["entries"][0]["status"] = None if mine is None else {"text": mine["text"], "emoji": mine["emoji"]}


save()
emit("hello")
sets = 0
for line in sys.stdin:
    request = json.loads(line)
    record["requests"].append(request)
    save()
    kind = request["type"]
    if kind == "fetch_recent":
        assert request["roomId"] == ROOM, request
        status["history"] = {"state": "snapshot", "roomId": ROOM, "rows": ROWS, "hasMore": False,
                             "category": "history_completeness_unknown"}
        emit(request_id=request["id"])
    elif kind == "fetch_recipients":
        assert request["roomId"] == ROOM, request
        status["recipients"] = {"state": "snapshot", "roomId": ROOM, "entries": ENTRIES, "agents": [],
                                "partial": False, "category": None}
        emit(request_id=request["id"])
    elif kind == "set_status":
        assert UUID.fullmatch(request["id"]), request
        assert set(request) <= {"version", "id", "type", "text", "emoji", "expiresInHours"}, request
        assert {"version", "id", "type", "text", "expiresInHours"} <= set(request), request
        sets += 1
        if sets == 2:
            refuse(request, "status_rate_limited")
            continue
        mine = status["userStatus"]["mine"]
        status["userStatus"] = {"state": "sending", "mine": mine, "category": None}
        emit(request_id=request["id"])
        if sets == 1:
            status["userStatus"] = {"state": "failed", "mine": mine, "category": "status_rejected"}
        else:
            mine = {"text": request["text"], "emoji": request.get("emoji"),
                    "expiresAt": int(time.time()) + request["expiresInHours"] * 3600}
            status["userStatus"] = {"state": "ready", "mine": mine, "category": None}
            mine_in_roster(mine)
        emit()
    elif kind == "clear_status":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "type", "version"], request
        status["userStatus"] = {"state": "sending", "mine": status["userStatus"]["mine"], "category": None}
        emit(request_id=request["id"])
        status["userStatus"] = {"state": "ready", "mine": None, "category": None}
        mine_in_roster(None)
        emit()
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
