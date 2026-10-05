#!/usr/bin/python3
"""Synthetic stdio helper for clickable links: no sockets, keys, relay, network or launcher.

It answers like the helper: a room whose messages mix links with hostile text,
and `open_link` requests that are recorded and acknowledged (nothing is ever
opened). A request in `agent` mode is refused with `link_agent_unconfigured`,
as the helper does when Omarchy has no default agent. Every request is recorded.
"""
import json
import os
import re
import sys
import time

INSTANCE = "links-fixture"
IDENTITY = "5" * 64
OTHER = "8" * 64
ROOM = "00000000-0000-4000-8000-0000000000c1"
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
CAPABILITIES = ["connection_status", "room_catalog", "room_history", "message_send", "message_actions", "open_link"]
assert sys.argv[1:] == ["ui-bridge"]
here = os.path.dirname(os.path.abspath(__file__))
record = {"requests": []}
MIXED, HOSTILE, PLAIN, IDN, QUERY = "a" * 64, "b" * 64, "c" * 64, "d" * 64, "e" * 64

TEXTS = {
    MIXED: "Docs at https://example.com/docs, and (https://example.com/a_(b)). Also see https://omarchy.org!",
    # Markup, a javascript: anchor, entities, other schemes and a right-to-left
    # override: only the two https links are links, everything else is literal text.
    HOSTILE: "<img src=\"https://evil.example/x.png\"> <a href=\"javascript:alert(1)\">click</a> &lt;b&gt;bold&lt;/b&gt; "
             "javascript:alert(1) file:///etc/passwd data:text/html,hi ftp://ftp.example/x mailto:a@example.com "
             "\u202etxt.exe https://ok.example/path",
    PLAIN: "no links here, just <b>markup</b> and &amp; things",
    IDN: "visit https://b\u00fccher.example/x https://\uff45xample.com/ https://cafe\u0301.example/ https://\u0430pple.com/ today https://ascii.example/ok",
    QUERY: "search https://example.com/?a=1&b=2 now",
}


def row(event_id, author, time, text):
    return {"id": event_id, "author": author, "time": time, "text": text, "edited": False, "truncated": False,
            "unavailable": False, "reactions": {"seen": 0, "working": 0, "chips": []}}


ROWS = [row(MIXED, OTHER, 1790000000, TEXTS[MIXED]), row(HOSTILE, OTHER, 1790000060, TEXTS[HOSTILE]),
        row(PLAIN, OTHER, 1790000120, TEXTS[PLAIN]), row(IDN, OTHER, 1790000180, TEXTS[IDN]),
        row(QUERY, IDENTITY, 1790000240, TEXTS[QUERY])]
status = {
    "connection": "authenticated", "identity": IDENTITY, "relay": "wss://fixture.example/", "generation": 3, "category": None,
    "catalog": {"state": "ready", "rooms": [{"id": ROOM, "name": "team", "description": "", "kind": "stream",
                                              "participants": [], "hidden": False}], "category": None},
    "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
    "recipients": {"state": "unavailable", "roomId": None, "entries": [], "partial": True, "category": None},
    "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None},
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


save()
emit("hello")
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
    elif kind == "open_link":
        assert sorted(request) == ["generation", "id", "instanceId", "mode", "type", "url", "version"], request
        assert UUID.fullmatch(request["id"]) and request["generation"] == 3 and request["instanceId"] == INSTANCE, request
        assert request["mode"] in ("browser", "floating", "agent"), request
        assert re.match(r"^https?://[^\s<>\"]+$", request["url"]), request
        if request["mode"] == "agent":
            time.sleep(0.5)  # a slow launcher: the next request is answered after this failure
            print(json.dumps({"version": 1, "type": "error", "id": request["id"], "category": "link_agent_unconfigured",
                              "instanceId": INSTANCE}), flush=True)
        else:
            emit(request_id=request["id"])
    elif kind in ("subscribe", "get_snapshot", "retry_connection", "fetch_recipients"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
