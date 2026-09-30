#!/usr/bin/python3
"""Synthetic stdio helper for panel onboarding: no sockets, keys, relay, or network.

Answers `set_relay` and `create_identity` the way the helper does: a refusal is
an error frame with a fixed category, success is a status frame carrying the
new relay or public identity. No secret exists anywhere in this fixture.
"""
import json
import os
import re
import sys

INSTANCE = "onboarding-fixture"
IDENTITY = "5" * 64
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "setup_assist"]
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
status = {"connection": "unconfigured", "identity": None, "relay": None, "generation": 1, "category": None}
creates = 0


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
        status.update(connection="authenticated")
        emit()
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
