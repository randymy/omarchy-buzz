#!/usr/bin/python3
"""Synthetic stdio helper for the post-join welcome: no sockets, keys, relay, or network.

Starts authenticated in a first community with two rooms. Joining a second
community by URL switches to it with an empty, partial catalog (as the live
relay reported), and its open rooms load after a short delay; joining one of
them adds it to the catalog. Switching between the two communities is a new
generation each time. No secret exists anywhere in this fixture.
"""
import json
import os
import re
import sys
import time

INSTANCE = "welcome-fixture"
IDENTITY = "7" * 64
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "setup_assist", "community_join", "communities"]
FIRST = "wss://first.example/"
SECOND = "wss://second.example/"
ROOMS = {
    FIRST: [("aaaaaaaa-0000-4000-8000-0000000000f1", "general"), ("aaaaaaaa-0000-4000-8000-0000000000f2", "random")],
    SECOND: [],
}
OPEN = [{"id": "bbbbbbbb-0000-4000-8000-0000000000b1", "name": "general", "description": "Everyone", "kind": "stream"},
        {"id": "bbbbbbbb-0000-4000-8000-0000000000b2", "name": "welcome-everyone", "description": "Say hello", "kind": "stream"}]
IDLE_SETUP = {"state": "idle", "inviteCode": None, "joinPolicy": None, "claim": None, "category": None}
NO_OPEN = {"state": "unavailable", "rooms": [], "category": None}
IDLE_ACTION = {"state": "idle", "action": None, "requestId": None, "roomId": None, "category": None}
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
entries = [[FIRST, "first"]]
open_rooms = [dict(room) for room in OPEN]


def catalog(relay):
    rooms = [{"id": room, "name": name, "description": "", "kind": "stream", "participants": [], "hidden": False}
             for room, name in ROOMS[relay]]
    # The second community's relay lists a partial catalog, like the live one.
    if relay == SECOND:
        return {"state": "partial", "rooms": rooms, "category": "room_catalog_partial"}
    return {"state": "ready", "rooms": rooms, "category": None}


status = {"connection": "authenticated", "identity": IDENTITY, "relay": FIRST, "generation": 1, "category": None,
          "catalog": catalog(FIRST), "setup": dict(IDLE_SETUP), "openRooms": dict(NO_OPEN),
          "roomAction": dict(IDLE_ACTION)}


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def communities():
    return {"state": "ready", "active": status["relay"], "category": None, "pendingInvite": False, "notice": None,
            "entries": [{"relay": relay, "name": name, "host": relay[6:-1], "active": relay == status["relay"], "hint": None}
                        for relay, name in entries]}


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE,
                      "generation": status["generation"], "capabilities": CAPABILITIES,
                      "status": dict(status, communities=communities())}), flush=True)


def activate(relay):
    status.update(relay=relay, generation=status["generation"] + 1, setup=dict(IDLE_SETUP),
                  openRooms=dict(NO_OPEN), roomAction=dict(IDLE_ACTION), catalog=catalog(relay))


def scoped(request, *keys):
    assert UUID.fullmatch(request["id"]), request
    assert sorted(request) == sorted(["version", "id", "type", "generation", "instanceId", *keys]), request
    assert request["instanceId"] == INSTANCE and request["generation"] == status["generation"], request


save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    assert request["version"] == 1
    record["requests"].append(request)
    save()
    kind = request["type"]
    if kind == "join_community":
        scoped(request, "input")
        assert request["input"] == "https://second.example" and len(entries) == 1, request
        entries.append([SECOND, "second"])
        activate(SECOND)
        emit(request_id=request["id"])
    elif kind == "switch_community":
        scoped(request, "relay")
        assert request["relay"] in (FIRST, SECOND) and request["relay"] != status["relay"], request
        activate(request["relay"])
        emit(request_id=request["id"])
    elif kind == "open_rooms":
        assert sorted(request) == ["id", "type", "version"] and status["relay"] == SECOND, request
        status["openRooms"] = {"state": "loading", "rooms": [], "category": None}
        emit(request_id=request["id"])
        # Long enough for the panel to show its loading line.
        time.sleep(0.8)
        status["openRooms"] = {"state": "snapshot", "rooms": [dict(room) for room in open_rooms], "category": None}
        emit()
    elif kind == "join_room":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "roomId", "type", "version"], request
        room = next(room for room in open_rooms if room["id"] == request["roomId"])
        action = {"action": "join", "requestId": request["id"], "roomId": room["id"], "category": None}
        status["roomAction"] = dict(action, state="sending")
        emit(request_id=request["id"])
        open_rooms.remove(room)
        ROOMS[SECOND].append((room["id"], room["name"]))
        status["roomAction"] = dict(action, state="acknowledged")
        status["catalog"] = catalog(SECOND)
        status["openRooms"] = {"state": "snapshot", "rooms": [dict(r) for r in open_rooms], "category": None}
        emit()
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
