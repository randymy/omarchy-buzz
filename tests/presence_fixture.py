#!/usr/bin/python3
"""Synthetic stdio helper for presence: no sockets, keys, relay, or network.

It answers like the helper: a room whose roster carries verified presence for
other members (online, away, unknown), two DMs (one partner offline, one
unknown), and this identity's presence view. Each `set_presence` is checked
for its exact shape, recorded, and answered with the state the helper would
derive and publish (as if the relay acknowledged it).
"""
import json
import os
import re
import sys
import time

INSTANCE = "presence-fixture"
IDENTITY = "5" * 64
ONLINE = "8" * 64
AWAY = "9" * 64
UNKNOWN = "7" * 64
DM_OFFLINE = "6" * 64
DM_UNKNOWN = "4" * 64
ROOM = "00000000-0000-4000-8000-0000000000c1"
DM_A = "00000000-0000-4000-8000-0000000000c2"
DM_B = "00000000-0000-4000-8000-0000000000c3"
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "room_history", "message_send", "room_recipients", "dm_open",
                "user_status", "presence"]
assert sys.argv[1:] == ["ui-bridge"]
here = os.path.dirname(os.path.abspath(__file__))
record = {"requests": []}


def row(n, author):
    return {"id": str(n) * 64, "author": author, "time": 1790000000 + n, "text": "Synthetic line " + str(n),
            "edited": False, "truncated": False, "unavailable": False}


ROWS = [row(1, ONLINE), row(2, AWAY), row(3, UNKNOWN), row(4, IDENTITY)]
PEERS = [{"key": DM_OFFLINE, "presence": "offline"}, {"key": ONLINE, "presence": "online"},
         {"key": AWAY, "presence": "away"}]
PEERS.sort(key=lambda p: p["key"])


def entries(own):
    return [{"key": IDENTITY, "name": "Fixture Me", "status": None, "presence": own},
            {"key": ONLINE, "name": "Robin", "status": None, "presence": "online"},
            {"key": AWAY, "name": "Sasha", "status": None, "presence": "away"},
            {"key": UNKNOWN, "name": "Quinn", "status": None, "presence": None}]


def dm(room, partner, name):
    return {"id": room, "name": name, "description": "", "kind": "dm", "participants": sorted([IDENTITY, partner]),
            "hidden": False}


status = {
    "connection": "authenticated", "identity": IDENTITY, "relay": "wss://fixture.example/", "generation": 1, "category": None,
    "catalog": {"state": "ready", "rooms": [
        {"id": ROOM, "name": "team", "description": "", "kind": "stream", "participants": [], "hidden": False},
        dm(DM_A, DM_OFFLINE, "Morgan"), dm(DM_B, DM_UNKNOWN, "Jules")], "category": None},
    "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
    "recipients": {"state": "unavailable", "roomId": None, "entries": [], "agents": [], "partial": True, "category": None},
    "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None},
    "dmOpen": {"state": "idle", "requestId": None, "channelId": None, "created": None, "category": None},
    "userStatus": {"state": "ready", "mine": None, "category": None},
    "presence": {"state": "unavailable", "mode": None, "published": None, "lastPublishedAt": None, "category": None,
                 "peers": []},
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


def derive(mode, active):
    return "offline" if mode == "offline" else "away" if mode == "away" or not active else "online"


save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    record["requests"].append(request)
    save()
    kind = request["type"]
    if kind == "fetch_recent":
        assert request["roomId"] in (ROOM, DM_A, DM_B), request
        status["history"] = {"state": "snapshot", "roomId": request["roomId"], "rows": ROWS if request["roomId"] == ROOM else [],
                             "hasMore": False, "category": "history_completeness_unknown"}
        emit(request_id=request["id"])
    elif kind == "fetch_recipients":
        assert request["roomId"] in (ROOM, DM_A, DM_B), request
        status["recipients"] = {"state": "snapshot", "roomId": request["roomId"], "entries": entries(status["presence"]["published"]),
                                "agents": [], "partial": False, "category": None}
        emit(request_id=request["id"])
    elif kind == "set_presence":
        assert UUID.fullmatch(request["id"]), request
        assert sorted(request) == ["active", "id", "mode", "type", "version"], request
        assert request["mode"] in ("auto", "away", "offline") and isinstance(request["active"], bool), request
        published = derive(request["mode"], request["active"])
        status["presence"] = {"state": "ready", "mode": request["mode"], "published": published,
                              "lastPublishedAt": int(time.time()), "category": None, "peers": PEERS}
        if status["recipients"]["state"] == "snapshot":
            status["recipients"]["entries"] = entries(published)
        emit(request_id=request["id"])
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
