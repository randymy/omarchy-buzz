#!/usr/bin/python3
"""Synthetic stdio helper for message edit, delete and reactions: no sockets, keys, relay, or network.

It answers like the helper: a room with this identity's messages and another
member's message carrying reaction chips. The first edit is refused by the
relay (`rejected`), the second is accepted; a delete and three reactions are
accepted; the last reaction's outcome is lost (`unknown`). Accepted actions
change the rows the next `fetch_recent` returns, exactly as the helper's
refreshed history would. Every request is recorded.
"""
import json
import os
import re
import sys

INSTANCE = "message-actions-fixture"
IDENTITY = "5" * 64
OTHER = "8" * 64
ROOM = "00000000-0000-4000-8000-0000000000c1"
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "room_history", "message_send", "message_actions"]
assert sys.argv[1:] == ["ui-bridge"]
here = os.path.dirname(os.path.abspath(__file__))
record = {"requests": []}
MINE, THEIRS, SHORT, GONE = "a" * 64, "b" * 64, "c" * 64, "d" * 64


def row(event_id, author, time, text, reactions=None, **extra):
    value = {"id": event_id, "author": author, "time": time, "text": text, "edited": False, "truncated": False,
             "unavailable": False, "reactions": reactions}
    value.update(extra)
    return value


ROWS = [
    row(MINE, IDENTITY, 1790000000, "hello team", {"seen": 0, "working": 0, "chips": []}),
    row(THEIRS, OTHER, 1790000060, "ship it", {"seen": 1, "working": 0, "chips": [
        {"emoji": "🎉", "count": 2, "mine": False}, {"emoji": "👍", "count": 1, "mine": True}]}),
    row(SHORT, IDENTITY, 1790000120, "a shortened message", {"seen": 0, "working": 0, "chips": []}, truncated=True),
    row(GONE, IDENTITY, 1790000180, "delete me", {"seen": 0, "working": 0, "chips": []}),
]
status = {
    "connection": "authenticated", "identity": IDENTITY, "relay": "wss://fixture.example/", "generation": 3, "category": None,
    "catalog": {"state": "ready", "rooms": [{"id": ROOM, "name": "team", "description": "", "kind": "stream",
                                              "participants": [], "hidden": False}], "category": None},
    "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
    "recipients": {"state": "unavailable", "roomId": None, "entries": [], "partial": True, "category": None},
    "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None},
}
counts = {}


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(here, "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE,
                      "generation": status["generation"], "capabilities": CAPABILITIES, "status": status}), flush=True)


def find(event_id):
    return next(r for r in ROWS if r["id"] == event_id)


def chips(event_id):
    return find(event_id)["reactions"]["chips"]


def deliver(request, outcome):
    status["delivery"] = {"requestId": request["id"], "roomId": ROOM, "eventId": None, "state": "sending", "category": None}
    emit()
    state, category = {"acknowledged": ("acknowledged", None), "rejected": ("rejected", "send_rejected"),
                       "unknown": ("unknown", "delivery_unknown")}[outcome]
    status["delivery"] = {"requestId": request["id"], "roomId": ROOM, "eventId": "e" * 64, "state": state, "category": category}
    emit()


def expect(request, keys):
    assert UUID.fullmatch(request["id"]), request
    assert sorted(request) == sorted(["version", "id", "type", "roomId", "eventId", "generation", "instanceId"] + keys), request
    assert request["roomId"] == ROOM and request["generation"] == 3 and request["instanceId"] == INSTANCE, request


save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    record["requests"].append(request)
    save()
    kind = request["type"]
    counts[kind] = counts.get(kind, 0) + 1
    if kind == "fetch_recent":
        assert request["roomId"] == ROOM, request
        status["history"] = {"state": "snapshot", "roomId": ROOM, "rows": ROWS, "hasMore": False,
                             "category": "history_completeness_unknown"}
        emit(request_id=request["id"])
    elif kind == "edit_message":
        expect(request, ["text"])
        assert request["eventId"] == MINE and request["text"] == "hello team (fixed)", request
        if counts[kind] == 1:
            deliver(request, "rejected")
        else:
            find(MINE).update(text=request["text"], edited=True)
            deliver(request, "acknowledged")
    elif kind == "delete_message":
        expect(request, [])
        assert request["eventId"] == GONE, request
        ROWS.remove(find(GONE))
        deliver(request, "acknowledged")
    elif kind == "add_reaction":
        expect(request, ["emoji"])
        assert request["eventId"] == THEIRS, request
        if request["emoji"] == "🎉":
            next(c for c in chips(THEIRS) if c["emoji"] == "🎉").update(count=3, mine=True)
            deliver(request, "acknowledged")
        elif request["emoji"] == "🔥":
            chips(THEIRS).append({"emoji": "🔥", "count": 1, "mine": True})
            deliver(request, "acknowledged")
        else:
            assert request["emoji"] == "😮", request
            deliver(request, "unknown")
    elif kind == "remove_reaction":
        expect(request, ["emoji"])
        assert request["eventId"] == THEIRS and request["emoji"] == "👍", request
        chips(THEIRS)[:] = [c for c in chips(THEIRS) if c["emoji"] != "👍"]
        deliver(request, "acknowledged")
    elif kind in ("subscribe", "get_snapshot", "retry_connection", "fetch_recipients"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
