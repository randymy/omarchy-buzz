#!/usr/bin/python3
"""Synthetic stdio helper serving older room pages: no sockets, keys, relay, or network."""
import json
import os
import sys

ROOM = "11111111-1111-4111-8111-111111111111"
INSTANCE = "older-history-fixture"
BASE = 1_700_000_000
ROOMS = [{"id": ROOM, "name": "Synthetic room", "description": "", "kind": "stream", "participants": [], "hidden": False}]
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}


def ident(n):
    return format(n + 1, "064x")


def row(n):
    return {"id": ident(n), "author": "a" * 64, "time": BASE + n, "text": "Synthetic message %d" % n,
            "edited": False, "truncated": False, "unavailable": False}


def history(first, cursor, older="idle", category="history_completeness_unknown"):
    return {"state": "snapshot", "roomId": ROOM, "rows": [row(n) for n in range(first, 100)],
            "hasMore": True, "category": category,
            "nextCursor": None if cursor is None else {"createdAt": BASE + cursor, "id": ident(cursor)},
            "olderState": older}


status = {"connection": "authenticated", "identity": "b" * 64,
          "relay": "wss://fixture.invalid", "generation": 3, "category": None,
          "catalog": {"state": "partial", "rooms": ROOMS, "category": None},
          "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None,
                      "nextCursor": None, "olderState": "idle"}}


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id,
                      "instanceId": INSTANCE, "generation": 3,
                      "capabilities": ["connection_status", "room_catalog", "room_history", "older_history"],
                      "status": status}), flush=True)


save()
emit("hello")
# The head holds rows 80..99; each older page adds the next 20 back to row 0,
# where the fixture reports the 100-row cap: older rows exist but are not held.
pages = [(60, 60, "history_completeness_unknown"), (0, None, "history_older_unheld")]
while True:
    line = sys.stdin.readline()
    if not line:
        break
    request = json.loads(line)
    kind = request["type"]
    if kind == "fetch_recent":
        assert request == {"version": 1, "id": request["id"], "type": "fetch_recent", "roomId": ROOM}
        status["history"] = history(80, 80)
        emit(request_id=request["id"])
    elif kind == "fetch_older":
        record["requests"].append(request)
        save()
        # The IPC reply precedes the helper's work, as in the real helper.
        emit(request_id=request["id"])
        first, cursor, category = pages.pop(0)
        status["history"]["olderState"] = "loading"
        emit()
        status["history"] = history(first, cursor, category=category)
        emit()
    elif kind in ("subscribe", "get_snapshot"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request")
