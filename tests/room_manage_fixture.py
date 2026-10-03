#!/usr/bin/python3
"""Synthetic stdio helper for room paging, creation and settings: no sockets,
keys, relay, or network.

Scripted like the relay it stands for: the first "Load more" times out and the
second lists the next page; the first create_room and the first update_room are
refused with a relay reason (permission refusals shown as said); later ones are
acknowledged. The viewer owns "general" and the room it creates, and is a plain
member of "ops".
"""
import json
import os
import re
import sys

INSTANCE = "room-manage-fixture"
ME = "7" * 64
ADA = "a" * 64
BOB = "b" * 64
PAT = "c" * 64
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "community_join", "room_manage"]
GENERAL = "aaaaaaaa-0000-4000-8000-000000000001"
OPS = "aaaaaaaa-0000-4000-8000-000000000002"
LATER = "aaaaaaaa-0000-4000-8000-000000000003"
DM = "aaaaaaaa-0000-4000-8000-000000000005"
CREATED = "bbbbbbbb-0000-4000-8000-000000000009"
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}


def room(room_id, name, about="", kind="stream", participants=()):
    return {"id": room_id, "name": name, "description": about, "kind": kind,
            "participants": list(participants), "hidden": False}


rooms = [room(GENERAL, "general", "Everyone"), room(OPS, "ops"), room(DM, "Pat", kind="dm", participants=[ME, PAT])]
more = {"state": "available", "category": None}
members = {
    GENERAL: [(ME, "owner", ""), (ADA, "admin", "Ada"), (BOB, "member", "")],
    OPS: [(ME, "member", ""), (ADA, "owner", "Ada")],
    CREATED: [(ME, "owner", "")],
}
roles = {GENERAL: "owner", OPS: "member", CREATED: "owner"}
topics = {GENERAL: "Welcome", OPS: "", CREATED: ""}
# Full descriptions; the catalog row carries only a shortened copy, as the helper's does.
abouts = {GENERAL: "Everyone", OPS: "", CREATED: ""}
ROW_ABOUT = 256
visibility = {GENERAL: "open", OPS: "open", CREATED: "open"}
room_action = {"state": "idle", "action": None, "requestId": None, "roomId": None, "category": None, "detail": None}
detail = {"state": "unavailable", "roomId": None, "topic": "", "visibility": "", "about": "", "aboutTruncated": False,
          "role": "", "members": [], "truncated": False, "category": None}
counts = {"more": 0, "create": 0, "update": 0, "polls": 0}
created = False


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def status():
    return {"connection": "authenticated", "identity": ME, "relay": "wss://fixture.example/", "generation": 1,
            "category": None,
            "catalog": {"state": "partial", "rooms": rooms, "category": "room_catalog_partial",
                        "more": more["state"], "moreCategory": more["category"]},
            "setup": {"state": "idle", "inviteCode": None, "joinPolicy": None, "claim": None, "category": None},
            "openRooms": {"state": "snapshot", "rooms": [], "category": None},
            "roomAction": room_action, "roomDetail": detail}


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE, "generation": 1,
                      "capabilities": CAPABILITIES, "status": status()}), flush=True)


def act(request, action, ok, reason=None):
    """The helper's two frames: `sending` (the reply), then the relay's answer."""
    global room_action
    target = request.get("roomId") or CREATED
    room_action = {"state": "sending", "action": action, "requestId": request["id"], "roomId": target,
                   "category": None, "detail": None}
    emit(request_id=request["id"])
    refused = {"create": "create_rejected", "details": "edit_rejected", "topic": "edit_rejected",
               "add_member": "member_add_rejected", "remove_member": "member_remove_rejected"}[action]
    room_action = {**room_action, "state": "acknowledged" if ok else "rejected",
                   "category": None if ok else refused, "detail": None if ok else reason}
    emit()


def refresh_detail(room_id):
    global detail
    detail = {"state": "snapshot", "roomId": room_id, "topic": topics[room_id], "visibility": visibility[room_id],
              "about": abouts[room_id], "aboutTruncated": False,
              "role": roles[room_id], "truncated": False, "category": None,
              "members": [{"key": k, "name": n, "role": r} for k, r, n in members[room_id]]}


save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    record["requests"].append(request)
    save()
    kind = request["type"]
    if kind == "load_more_rooms":
        counts["more"] += 1
        more.update(state="loading", category=None)
        emit()
        if counts["more"] == 1:
            more.update(state="failed", category="room_catalog_timeout")
        else:
            rooms.append(room(LATER, "later"))
            more.update(state="none", category=None)
        emit()
    elif kind == "refresh_rooms":
        counts["polls"] += 1
        if created and not any(r["id"] == CREATED for r in rooms):
            rooms.append(room(CREATED, "Plans", abouts[CREATED][:ROW_ABOUT]))
        emit()
    elif kind == "fixture_rename":
        # Another client renames the created room and changes its topic.
        for r in rooms:
            if r["id"] == CREATED:
                r["name"] = "Renamed elsewhere"
        topics[CREATED] = "Other topic"
        if detail["roomId"] == CREATED:
            refresh_detail(CREATED)
        emit()
    elif kind == "fetch_room_detail":
        assert UUID.fullmatch(request["roomId"]) and sorted(request) == ["id", "roomId", "type", "version"], request
        if request["roomId"] in members:
            refresh_detail(request["roomId"])
        emit(request_id=request["id"])
    elif kind == "create_room":
        assert UUID.fullmatch(request["id"]) and request["visibility"] in ("open", "private"), request
        assert sorted(request) == ["about", "id", "name", "type", "version", "visibility"], request
        counts["create"] += 1
        if counts["create"] == 1:
            act(request, "create", False, "blocked: you may not create rooms here")
        else:
            created = True
            visibility[CREATED] = request["visibility"]
            abouts[CREATED] = request["about"]
            act(request, "create", True)
    elif kind == "update_room":
        # Only what changed is sent: at least one of name and about, never a copy of the rest.
        assert set(request) <= {"about", "id", "name", "roomId", "type", "version"} and set(request) & {"about", "name"}, request
        counts["update"] += 1
        if counts["update"] == 1:
            act(request, "details", False, "restricted: actor not authorized for name/about changes")
        else:
            for r in rooms:
                if r["id"] == request["roomId"]:
                    r["name"] = request.get("name", r["name"])
                    if "about" in request:
                        abouts[r["id"]] = request["about"]
                    r["description"] = abouts[r["id"]][:ROW_ABOUT]
            act(request, "details", True)
    elif kind == "set_room_topic":
        assert sorted(request) == ["id", "roomId", "topic", "type", "version"], request
        topics[request["roomId"]] = request["topic"]
        act(request, "topic", True)
        refresh_detail(request["roomId"])
        emit()
    elif kind == "add_room_member":
        assert sorted(request) == ["id", "key", "roomId", "type", "version"] and re.fullmatch("[a-f0-9]{64}", request["key"]), request
        members[request["roomId"]].append((request["key"], "member", "Pat" if request["key"] == PAT else ""))
        act(request, "add_member", True)
        refresh_detail(request["roomId"])
        emit()
    elif kind == "remove_room_member":
        assert sorted(request) == ["id", "key", "roomId", "type", "version"], request
        members[request["roomId"]] = [m for m in members[request["roomId"]] if m[0] != request["key"]]
        act(request, "remove_member", True)
        refresh_detail(request["roomId"])
        emit()
    elif kind in ("subscribe", "get_snapshot", "retry_connection", "open_rooms"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
