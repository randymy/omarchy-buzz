#!/usr/bin/python3
"""Synthetic stdio helper for panel onboarding: no sockets, keys, relay, or network.

Answers `set_relay` and `create_identity` the way the helper does: a refusal is
an error frame with a fixed category, success is a status frame carrying the
new relay or public identity. The new identity is refused by the relay (not a
member yet); an invite is then redeemed (terms shown, accepted, claimed), the
helper connects with no rooms, and an open room is joined and left. No secret
exists anywhere in this fixture.
"""
import json
import os
import re
import sys

INSTANCE = "onboarding-fixture"
IDENTITY = "5" * 64
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "setup_assist", "community_join"]
CODE = "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
OPEN = {"id": "aaaaaaaa-0000-4000-8000-00000000000a", "name": "welcome", "description": "Say hello", "kind": "stream"}
TERMS = "Be kind to one another."
NO_CATALOG = {"state": "unavailable", "rooms": [], "category": None}
IDLE_SETUP = {"state": "idle", "inviteCode": None, "joinPolicy": None, "claim": None, "category": None}
NO_OPEN = {"state": "unavailable", "rooms": [], "category": None}
IDLE_ACTION = {"state": "idle", "action": None, "requestId": None, "roomId": None, "category": None}
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
status = {"connection": "unconfigured", "identity": None, "relay": None, "generation": 1, "category": None,
          "catalog": dict(NO_CATALOG), "setup": dict(IDLE_SETUP), "openRooms": dict(NO_OPEN),
          "roomAction": dict(IDLE_ACTION)}
creates = 0
leaves = 0


def action(state, request, room, category=None):
    return {"state": state, "action": request["type"].split("_")[0], "requestId": request["id"],
            "roomId": room, "category": category}


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE,
                      "generation": status["generation"], "capabilities": CAPABILITIES,
                      "status": status}), flush=True)


def refuse(request_id, category):
    print(json.dumps({"version": 1, "type": "error", "id": request_id,
                      "instanceId": INSTANCE, "category": category}), flush=True)


save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    assert request["version"] == 1
    record["requests"].append(request)
    save()
    kind = request["type"]
    if kind == "set_relay":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "type", "url", "version"], request
        assert status["connection"] != "authenticated"
        if request["url"] == "wss://refused.example":
            refuse(request["id"], "setup_invalid_relay")
            continue
        assert request["url"] == "wss://fixture.example", request
        status.update(relay="wss://fixture.example/", identity=None, generation=status["generation"] + 1)
        emit(request_id=request["id"])
    elif kind == "create_identity":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "type", "version"], request
        assert status["relay"] and status["identity"] is None
        creates += 1
        if creates == 1:
            refuse(request["id"], "relay_unavailable")
            continue
        status.update(identity=IDENTITY, generation=status["generation"] + 1)
        emit(request_id=request["id"])
        status.update(connection="connecting")
        emit()
        # Not a relay member yet: the relay refuses authentication.
        status.update(connection="disconnected", category="auth_rejected")
        emit()
    elif kind == "claim_invite":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "input", "type", "version"], request
        assert status["connection"] == "disconnected" and status["identity"] == IDENTITY
        if "other.example" in request["input"]:
            status["setup"] = {**IDLE_SETUP, "state": "failed", "category": "invite_relay_mismatch"}
            emit()
            refuse(request["id"], "invite_relay_mismatch")
            continue
        assert request["input"] == "https://fixture.example/invite/" + CODE, request
        status["setup"] = {**IDLE_SETUP, "state": "policy", "inviteCode": CODE,
                           "joinPolicy": {"text": TERMS, "version": "v1", "ageRequired": True, "truncated": False}}
        emit(request_id=request["id"])
    elif kind == "accept_invite":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["code", "id", "policyVersion", "type", "version"], request
        assert request["code"] == CODE and request["policyVersion"] == "v1", request
        status["setup"] = {**IDLE_SETUP, "state": "joined", "claim": {
            "status": "joined", "communityId": "11111111-1111-4111-8111-111111111111", "host": "fixture.example", "role": "member"}}
        emit(request_id=request["id"])
        status.update(connection="connecting", category=None)
        emit()
        status.update(connection="authenticated", catalog={"state": "partial", "rooms": [], "category": "room_catalog_partial"})
        emit()
    elif kind == "open_rooms":
        assert sorted(request) == ["id", "type", "version"] and status["connection"] == "authenticated", request
        status["openRooms"] = {"state": "snapshot", "rooms": [dict(OPEN)], "category": None}
        emit(request_id=request["id"])
    elif kind in ("join_room", "leave_room"):
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "roomId", "type", "version"], request
        assert request["roomId"] == OPEN["id"], request
        status["roomAction"] = action("sending", request, OPEN["id"])
        emit(request_id=request["id"])
        room = {"id": OPEN["id"], "name": OPEN["name"], "description": OPEN["description"], "kind": "stream",
                "participants": [], "hidden": False}
        if kind == "join_room":
            status["roomAction"] = action("acknowledged", request, OPEN["id"])
            status["catalog"] = {"state": "partial", "rooms": [room], "category": "room_catalog_partial"}
            status["openRooms"] = {"state": "snapshot", "rooms": [], "category": None}
        else:
            leaves += 1
            if leaves == 1:
                status["roomAction"] = action("rejected", request, OPEN["id"], "leave_rejected")
            else:
                status["roomAction"] = action("acknowledged", request, OPEN["id"])
                status["catalog"] = {"state": "partial", "rooms": [], "category": "room_catalog_partial"}
                status["openRooms"] = {"state": "snapshot", "rooms": [dict(OPEN)], "category": None}
        emit()
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
