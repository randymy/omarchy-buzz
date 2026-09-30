#!/usr/bin/python3
"""Synthetic stdio helper for Settings → Invite people: no sockets, keys, relay, or network.

The first `mint_invite` is refused as the relay refuses a member that is not
an owner or admin (status `failed`/`invite_forbidden`, then the error frame,
as the helper answers); the second mints a fixed synthetic v2 code.
"""
import json
import os
import re
import sys
import time

INSTANCE = "invites-fixture"
IDENTITY = "5" * 64
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "invite_mint"]
CODE = "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
IDLE = {"state": "idle", "code": None, "expiresAt": None, "maxUses": None, "role": None, "category": None}
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
status = {"connection": "authenticated", "identity": IDENTITY, "relay": "wss://fixture.example/", "generation": 1,
          "category": None, "catalog": {"state": "ready", "rooms": [], "category": None}, "invites": dict(IDLE)}
mints = 0


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


save()
emit("hello")
for line in sys.stdin:
    request = json.loads(line)
    record["requests"].append(request)
    save()
    kind = request["type"]
    if kind == "mint_invite":
        assert UUID.fullmatch(request["id"]), request
        assert sorted(request) == ["expiresInHours", "id", "maxUses", "type", "version"], request
        mints += 1
        status["invites"] = {**IDLE, "state": "minting"}
        emit()
        if mints == 1:
            status["invites"] = {**IDLE, "state": "failed", "category": "invite_forbidden"}
            emit()
            print(json.dumps({"version": 1, "type": "error", "id": request["id"], "instanceId": INSTANCE,
                              "category": "invite_forbidden"}), flush=True)
            continue
        status["invites"] = {"state": "minted", "code": CODE, "expiresAt": int(time.time()) + request["expiresInHours"] * 3600,
                             "maxUses": request["maxUses"], "role": "member", "category": None}
        emit(request_id=request["id"])
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
