#!/usr/bin/python3
"""Synthetic stdio helper for communities: no sockets, keys, relay, or network.

Answers `join_community`, `switch_community`, `rename_community` and
`leave_community` the way the helper does: a refusal is an error frame with a
fixed category (the status view carries the same category), success is a
status frame carrying the saved list, and a change of the active community is
a new generation with the old relay's state cleared. First setup saves the
invite link's relay without an identity (`pendingInvite`); the panel creates
the identity and redeems the invite. A community with terms is switched to
and left awaiting `accept_invite`. No secret exists anywhere in this fixture.

Run as `agents-bridge` it is the agent service instead: one agent enrolled in
the first community (with a direct message there), one in the second, and one
in both (the second first); its `activeRelay` is the relay the helper side
last reported (a file beside the send record), re-read on every `subscribe`.
Besides `subscribe` it answers `enroll_agent_in` (vClaude added to the second
community) and `leave_agent_community` (the agent in both leaves the second).
"""
import hashlib
import json
import os
import re
import sys

INSTANCE = "communities-fixture"
IDENTITY = "6" * 64
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "setup_assist", "community_join", "communities"]
CODE = "v2.AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
FIRST_LINK = "https://first.example/invite/" + CODE
THIRD_LINK = "buzz://join?relay=wss://third.example&code=" + CODE
TERMS = "Third team terms."
ROOMS = {
    "wss://first.example/": [("aaaaaaaa-0000-4000-8000-0000000000f1", "general"),
                             ("aaaaaaaa-0000-4000-8000-0000000000f2", "random")],
    "wss://second.example/": [("bbbbbbbb-0000-4000-8000-0000000000b1", "night-shift")],
    "wss://third.example/": [("cccccccc-0000-4000-8000-0000000000c1", "welcome")],
}
NAMES = {"wss://first.example/": "first", "wss://second.example/": "second", "wss://third.example/": "third"}
HINTS = {"wss://second.example/": "Second Team HQ"}
IDLE_SETUP = {"state": "idle", "inviteCode": None, "joinPolicy": None, "claim": None, "category": None}
NO_OPEN = {"state": "unavailable", "rooms": [], "category": None}
IDLE_ACTION = {"state": "idle", "action": None, "requestId": None, "roomId": None, "category": None}
NO_CATALOG = {"state": "unavailable", "rooms": [], "category": None}
VCLAUDE = "33333333-3333-4333-8333-333333333333"
NIGHT_BOT = "44444444-4444-4444-8444-444444444444"
VCLAUDE_KEY = "c" * 64
NIGHT_KEY = "d" * 64
BOTH = "77777777-7777-4777-8777-777777777777"
BOTH_KEY = "e" * 64
DM_ROOM = "dddddddd-0000-4000-8000-0000000000d1"


def side_file(name):
    return os.path.join(os.path.dirname(os.environ["BUZZ_SEND_RECORD"]), name)


def agents_bridge():
    instance = "communities-agents"
    requests = []

    def persona(agent_id, name, key, relay, community, rooms):
        return {"id": agent_id, "name": name, "description": "", "instructions": "", "harness": "codex", "model": "",
                "acpCommand": "buzz-acp", "rooms": rooms, "respondTo": "owner-only",
                "workspace": "/home/fixture/.local/state/omarchy-buzz-room-workspaces/" + agent_id, "identity": key,
                "enrolled": True, "unit": "inactive", "startAtLogin": False, "answersDms": True, "published": True,
                "lastError": None, "relay": relay, "community": community,
                "instances": [instance_view(agent_id, relay, community, rooms, True)]}

    def instance_view(agent_id, relay, community, rooms, first):
        suffix = "" if first else "-" + hashlib.sha256(relay.encode()).hexdigest()[:12]
        return {"relay": relay, "community": community, "rooms": rooms,
                "unit": "omarchy-buzz-agent-%s%s.service" % (agent_id, suffix), "startAtLogin": False,
                "published": True, "lastError": None, "state": "inactive"}

    first_room, second_room = ROOMS["wss://first.example/"][0][0], ROOMS["wss://second.example/"][0][0]
    agents = [persona(VCLAUDE, "vClaude", VCLAUDE_KEY, "wss://first.example/", "first", [first_room]),
              persona(NIGHT_BOT, "Night bot", NIGHT_KEY, "wss://second.example/", "second", [second_room]),
              persona(BOTH, "Both bot", BOTH_KEY, "wss://second.example/", "second", [second_room])]
    # Enrolled in the first community as well, after the second.
    agents[2]["instances"].append(instance_view(BOTH, "wss://first.example/", "first", [ROOMS["wss://first.example/"][1][0]], False))
    pending = {"value": None}

    def emit(kind="status", request_id=None):
        try:
            with open(side_file("active-relay.json"), encoding="utf-8") as source:
                active = json.load(source)
        except FileNotFoundError:
            active = None
        print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": instance,
                          "capabilities": ["agent_manager"],
                          "status": {"activeRelay": active,
                                     "harnesses": [{"id": "codex", "bundle": "ready", "signedIn": True}],
                                     "agents": agents, "pending": pending["value"],
                                     "modelProbe": {"agentId": None, "state": "idle", "model": "", "detail": None}}}),
              flush=True)

    def finish(request):
        pending["value"] = {"requestId": request["id"], "type": request["type"], "state": "done", "category": None,
                            "detail": None}
        emit(request_id=request["id"])

    emit("hello")
    for line in sys.stdin:
        request = json.loads(line)
        assert request["instanceId"] == instance, request
        requests.append(request)
        with open(side_file("agents-record.json.tmp"), "w", encoding="utf-8") as output:
            json.dump({"requests": requests}, output)
        os.replace(side_file("agents-record.json.tmp"), side_file("agents-record.json"))
        if request["type"] == "subscribe":
            assert sorted(request) == ["id", "instanceId", "type", "version"], request
            emit(request_id=request["id"])
        elif request["type"] == "enroll_agent_in":
            assert sorted(request) == ["agentId", "id", "instanceId", "relay", "rooms", "type", "version"], request
            assert request["agentId"] == VCLAUDE and request["relay"] == "wss://second.example/", request
            assert request["rooms"] == [second_room], request
            agents[0]["instances"].append(instance_view(VCLAUDE, request["relay"], "second", request["rooms"], False))
            finish(request)
        elif request["type"] == "leave_agent_community":
            assert sorted(request) == ["agentId", "id", "instanceId", "relay", "type", "version"], request
            assert request["agentId"] == BOTH and request["relay"] == "wss://second.example/", request
            # The first instance left: the other one (first) repeats at the top level.
            remaining = agents[2]["instances"][1]
            agents[2].update(relay=remaining["relay"], community=remaining["community"], rooms=remaining["rooms"],
                             instances=[remaining])
            finish(request)
        else:
            raise AssertionError("Unexpected agent request %s" % request["type"])


if sys.argv[1:] == ["agents-bridge"]:
    agents_bridge()
    sys.exit(0)
assert sys.argv[1:] == ["ui-bridge"]
record = {"requests": []}
entries = []  # [relay, name]
view = {"state": "ready", "category": None, "pendingInvite": False, "notice": None}
status = {"connection": "unconfigured", "identity": None, "relay": None, "generation": 1, "category": None,
          "catalog": dict(NO_CATALOG), "setup": dict(IDLE_SETUP), "openRooms": dict(NO_OPEN),
          "roomAction": dict(IDLE_ACTION)}
leaves = 0


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(os.path.dirname(os.path.abspath(__file__)), "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def communities():
    return {"state": view["state"], "active": status["relay"], "category": view["category"],
            "pendingInvite": view["pendingInvite"], "notice": view["notice"],
            "entries": [{"relay": relay, "name": name, "host": relay[6:-1], "active": relay == status["relay"],
                         "hint": HINTS.get(relay)} for relay, name in entries]}


def emit(kind="status", request_id=None):
    # What the agent service would read from the configuration.
    with open(side_file("active-relay.json.tmp"), "w", encoding="utf-8") as output:
        json.dump(status["relay"], output)
    os.replace(side_file("active-relay.json.tmp"), side_file("active-relay.json"))
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE,
                      "generation": status["generation"], "capabilities": CAPABILITIES,
                      "status": dict(status, communities=communities())}), flush=True)


def refuse(request_id, category):
    view.update(state="failed", category=category)
    emit()
    print(json.dumps({"version": 1, "type": "error", "id": request_id,
                      "instanceId": INSTANCE, "category": category}), flush=True)


def ready(**extra):
    view.update(state="ready", category=None, pendingInvite=False, notice=None)
    view.update(extra)


def rooms(relay):
    listed = [{"id": room, "name": name, "description": "", "kind": "stream", "participants": [], "hidden": False}
              for room, name in ROOMS[relay]]
    # The direct message with vClaude exists only in its own community.
    if relay == "wss://first.example/":
        listed.append({"id": DM_ROOM, "name": "vClaude", "description": "", "kind": "dm",
                       "participants": sorted([IDENTITY, VCLAUDE_KEY]), "hidden": False})
    return {"state": "ready", "category": None, "rooms": listed}


def activate(relay, connection="authenticated", category=None):
    """A new active community: new generation, old relay's state cleared."""
    status.update(relay=relay, generation=status["generation"] + 1, connection=connection, category=category,
                  setup=dict(IDLE_SETUP), openRooms=dict(NO_OPEN), roomAction=dict(IDLE_ACTION),
                  catalog=rooms(relay) if connection == "authenticated" else dict(NO_CATALOG))


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
        text = request["input"]
        if text == "not a link":
            refuse(request["id"], "join_invalid")
        elif text == "https://refused.example":
            refuse(request["id"], "join_rejected")
        elif text == FIRST_LINK:
            assert status["identity"] is None and status["connection"] == "unconfigured", status
            entries[:] = [["wss://first.example/", "first"]]
            ready(pendingInvite=True)
            status.update(relay="wss://first.example/", generation=status["generation"] + 1)
            emit(request_id=request["id"])
        elif text == "https://second.example":
            assert status["connection"] == "authenticated", status
            entries.append(["wss://second.example/", "second"])
            ready()
            activate("wss://second.example/")
            emit(request_id=request["id"])
        elif text == THIRD_LINK:
            entries.append(["wss://third.example/", "third"])
            ready()
            # Not a member until the terms are accepted: the relay refuses sign-in.
            activate("wss://third.example/", "disconnected", "auth_rejected")
            status["setup"] = {**IDLE_SETUP, "state": "policy", "inviteCode": CODE,
                               "joinPolicy": {"text": TERMS, "version": "t1", "ageRequired": False, "truncated": False}}
            emit(request_id=request["id"])
        else:
            raise AssertionError("Unexpected join input " + text)
    elif kind == "create_identity":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "type", "version"], request
        status.update(identity=IDENTITY, generation=status["generation"] + 1)
        ready()
        emit(request_id=request["id"])
        status.update(connection="disconnected", category="auth_rejected")
        emit()
    elif kind == "claim_invite":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "input", "type", "version"], request
        assert request["input"] == FIRST_LINK and status["relay"] == "wss://first.example/", request
        status["setup"] = {**IDLE_SETUP, "state": "policy", "inviteCode": CODE, "joinPolicy": None}
        emit(request_id=request["id"])
    elif kind == "accept_invite":
        assert UUID.fullmatch(request["id"]) and request["code"] == CODE, request
        relay = status["relay"]
        assert request["policyVersion"] == ("t1" if relay == "wss://third.example/" else None), request
        status["setup"] = {**IDLE_SETUP, "state": "joined", "claim": {
            "status": "joined", "communityId": "11111111-1111-4111-8111-111111111111", "host": relay[6:-1], "role": "member"}}
        emit(request_id=request["id"])
        status.update(connection="authenticated", category=None, catalog=rooms(relay))
        emit()
    elif kind == "switch_community":
        scoped(request, "relay")
        assert any(relay == request["relay"] for relay, _ in entries) and request["relay"] != status["relay"], request
        ready()
        activate(request["relay"])
        emit(request_id=request["id"])
    elif kind == "rename_community":
        assert UUID.fullmatch(request["id"]) and sorted(request) == ["id", "name", "relay", "type", "version"], request
        assert request["name"] == "Night shift", request
        for entry in entries:
            if entry[0] == request["relay"]:
                entry[1] = request["name"]
        ready()
        emit(request_id=request["id"])
    elif kind == "leave_community":
        scoped(request, "relay")
        assert len(entries) > 1, "the panel offered to leave the last community"
        leaves += 1
        index = [relay for relay, _ in entries].index(request["relay"])
        del entries[index]
        # The second leave: the relay no longer had this member.
        ready(notice="already_absent" if leaves == 2 else None)
        if request["relay"] == status["relay"]:
            activate(entries[min(index, len(entries) - 1)][0])
        emit(request_id=request["id"])
    elif kind == "open_rooms":
        status["openRooms"] = {"state": "snapshot", "rooms": [], "category": None}
        emit(request_id=request["id"])
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
