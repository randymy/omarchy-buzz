#!/usr/bin/python3
"""Synthetic stdio helper for file attachments: no sockets, keys, relay, or network.

It answers like the helper: a room snapshot whose first message carries a PDF
and an image and whose second has a malformed attachment; a verified preview
file for the image (a PNG it writes next to the record); a download of the
PDF that progresses and completes, one of the image that fails verification;
uploads (a refused SVG, then two files that become pending attachments);
removal; and a send that carries the draft's attachments and clears them.
"""
import json
import os
import re
import struct
import sys
import zlib

INSTANCE = "attachments-fixture"
IDENTITY = "7" * 64
ROOM = "00000000-0000-4000-8000-0000000000a1"
ORIGIN = "https://fixture.example"
UUID = re.compile(r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$")
HEX = re.compile(r"^[a-f0-9]{64}$")
CAPABILITIES = ["connection_status", "room_catalog", "room_history", "message_send", "attachments"]
PDF_HASH, PNG_HASH, NOTES_HASH, SHOT_HASH = "1" * 64, "2" * 64, "3" * 64, "4" * 64
ROW_A, ROW_B = "a" * 64, "b" * 64
assert sys.argv[1:] == ["ui-bridge"]
here = os.path.dirname(os.path.abspath(__file__))
record = {"requests": []}


def attachment(name, mime, size, digest, ext, kind, dim=None):
    return {"name": name, "mime": mime, "size": size, "url": f"{ORIGIN}/media/{digest}.{ext}",
            "hash": digest, "dim": dim, "kind": kind}


PDF = attachment("report.pdf", "application/pdf", 12345, PDF_HASH, "pdf", "file")
PNG = attachment("shot.png", "image/png", 2048, PNG_HASH, "png", "image", "16x12")
ROWS = [
    {"id": ROW_A, "author": "8" * 64, "time": 1790000000, "text": "Here is the report", "edited": False,
     "truncated": False, "unavailable": False, "attachments": [PDF, PNG], "attachmentsUnavailable": False},
    {"id": ROW_B, "author": "8" * 64, "time": 1790000060, "text": "Broken metadata", "edited": False,
     "truncated": False, "unavailable": False, "attachments": [], "attachmentsUnavailable": True},
]
IDLE_DOWNLOAD = {"state": "idle", "eventId": None, "hash": None, "path": None, "received": 0, "size": None, "category": None}
IDLE_UPLOAD = {"state": "idle", "scope": None, "name": None, "category": None}
status = {
    "connection": "authenticated", "identity": IDENTITY, "relay": "wss://fixture.example/", "generation": 1, "category": None,
    "catalog": {"state": "ready", "rooms": [{"id": ROOM, "name": "files", "description": "", "kind": "stream",
                                              "participants": [], "hidden": False}], "category": None},
    "history": {"state": "unavailable", "roomId": None, "rows": [], "hasMore": None, "category": None},
    "delivery": {"requestId": None, "roomId": None, "eventId": None, "state": "idle", "category": None},
    "download": dict(IDLE_DOWNLOAD), "thumbnails": [], "pendingAttachments": [], "upload": dict(IDLE_UPLOAD),
}


def png(width, height):
    raw = b"".join(b"\x00" + b"\xd0\x40\x40" * width for _ in range(height))
    chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def save():
    path = os.environ["BUZZ_SEND_RECORD"]
    assert path == os.path.join(here, "send-record.json")
    with open(path + ".tmp", "w", encoding="utf-8") as output:
        json.dump(record, output)
    os.replace(path + ".tmp", path)


def emit(kind="status", request_id=None):
    print(json.dumps({"version": 1, "type": kind, "id": request_id, "instanceId": INSTANCE,
                      "generation": status["generation"], "capabilities": CAPABILITIES, "status": status}), flush=True)


def refuse(request, category):
    print(json.dumps({"version": 1, "type": "error", "id": request["id"], "instanceId": INSTANCE,
                      "category": category}), flush=True)


save()
emit("hello")
uploads = 0
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
    elif kind == "thumbnail_attachment":
        assert sorted(request) == ["eventId", "hash", "id", "type", "version"], request
        assert (request["eventId"], request["hash"]) == (ROW_A, PNG_HASH), request
        thumbs = os.path.join(here, "thumbs")
        os.makedirs(thumbs, mode=0o700, exist_ok=True)
        path = os.path.join(thumbs, PNG_HASH + ".png")
        with open(path, "wb") as output:
            output.write(png(16, 12))
        status["thumbnails"] = [{"hash": PNG_HASH, "path": path}]
        emit(request_id=request["id"])
    elif kind == "download_attachment":
        assert sorted(request) == ["eventId", "hash", "id", "type", "version"], request
        assert request["eventId"] == ROW_A and request["hash"] in (PDF_HASH, PNG_HASH), request
        size = PDF["size"] if request["hash"] == PDF_HASH else PNG["size"]
        view = {"state": "downloading", "eventId": ROW_A, "hash": request["hash"], "path": None, "received": 0,
                "size": size, "category": None}
        status["download"] = dict(view)
        emit(request_id=request["id"])
        status["download"] = {**view, "received": 5000 if request["hash"] == PDF_HASH else 1024}
        emit()
        if request["hash"] == PDF_HASH:
            status["download"] = {**view, "state": "done", "received": size,
                                  "path": "/home/fixture/Downloads/report.pdf"}
        else:
            status["download"] = {**view, "state": "failed", "category": "attachment_mismatch"}
        emit()
    elif kind == "open_download":
        assert sorted(request) == ["id", "path", "type", "version"], request
        assert request["path"] == "/home/fixture/Downloads/report.pdf", request
        emit(request_id=request["id"])
    elif kind == "upload_attachment":
        assert sorted(request) == ["id", "path", "roomId", "type", "version"], request
        assert request["roomId"] == ROOM, request
        uploads += 1
        if request["path"].endswith(".svg"):
            status["upload"] = {"state": "failed", "scope": ROOM, "name": None, "category": "attachment_type_refused"}
            emit()
            refuse(request, "attachment_type_refused")
            continue
        name = os.path.basename(request["path"])
        digest, mime, ext, size = ((NOTES_HASH, "application/pdf", "pdf", 3210) if name == "notes.pdf"
                                   else (SHOT_HASH, "image/png", "png", 999))
        status["upload"] = {"state": "uploading", "scope": ROOM, "name": name, "category": None}
        emit(request_id=request["id"])
        status["pendingAttachments"] = status["pendingAttachments"] + [
            {"scope": ROOM, "name": name, "mime": mime, "size": size, "url": f"{ORIGIN}/media/{digest}.{ext}",
             "hash": digest, "dim": None}]
        status["upload"] = {"state": "done", "scope": ROOM, "name": name, "category": None}
        emit()
    elif kind == "remove_pending_attachment":
        assert sorted(request) == ["hash", "id", "type", "version"], request
        before = len(status["pendingAttachments"])
        status["pendingAttachments"] = [p for p in status["pendingAttachments"] if p["hash"] != request["hash"]]
        assert len(status["pendingAttachments"]) == before - 1, request
        emit(request_id=request["id"])
    elif kind == "send_message":
        assert UUID.fullmatch(request["id"]) and request["roomId"] == ROOM, request
        assert [p["hash"] for p in status["pendingAttachments"]] == [NOTES_HASH], status["pendingAttachments"]
        status["delivery"] = {"requestId": request["id"], "roomId": ROOM, "eventId": None, "state": "sending", "category": None}
        emit(request_id=request["id"])
        status["delivery"] = {**status["delivery"], "eventId": "e" * 64, "state": "acknowledged"}
        status["pendingAttachments"] = []
        emit()
    elif kind in ("subscribe", "get_snapshot", "retry_connection"):
        emit(request_id=request["id"])
    else:
        raise AssertionError("Unexpected synthetic request " + kind)
