#!/usr/bin/python3
"""Scripted Codex app-server peer: no credentials, threads, turns, or network."""

import json
import os
from pathlib import Path
import sys


STATE = Path(os.environ["CODEX_ROUTING_STATE"])
GATEWAY = "https://routing-probe.invalid/v1"
MAX_LINE = 16 * 1024
ALLOWED = {"initialize", "account/read", "config/read", "thread/start"}


def reply(request_id, *, result=None, error=None):
    value = {"id": request_id}
    value["result" if error is None else "error"] = result if error is None else error
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def record(capture):
    # State contains fixed booleans only, never a request, URL, path, or token.
    with STATE.open("a", encoding="ascii") as output:
        output.write(json.dumps(capture, separators=(",", ":")) + "\n")


def main():
    if sys.argv[1:] != ["app-server"]:
        return 2
    count = 0
    while True:
        line = sys.stdin.buffer.readline(MAX_LINE + 1)
        if not line:
            return 0
        if len(line) > MAX_LINE:
            return 3
        try:
            message = json.loads(line)
        except (UnicodeError, ValueError):
            return 4
        if not isinstance(message, dict):
            return 5
        method = message.get("method")
        request_id = message.get("id")
        if method not in ALLOWED or type(request_id) is not int or not isinstance(message.get("params"), dict):
            return 6
        params = message["params"]
        if method == "initialize":
            reply(request_id, result={"userAgent": "scripted-routing-peer",
                                      "codexHome": os.environ["CODEX_HOME"],
                                      "platformFamily": "unix", "platformOs": "linux"})
        elif method == "account/read":
            # Synthetic no-auth fixture response. It asserts no real account state.
            reply(request_id, result={"account": None, "requiresOpenaiAuth": False})
        elif method == "config/read":
            reply(request_id, result={"config": {}, "origins": {}, "layers": None})
        else:
            count += 1
            if count > 2:
                return 7
            config = params.get("config")
            custom = config.get("model_providers", {}).get("custom-gateway") if isinstance(config, dict) and isinstance(config.get("model_providers"), dict) else None
            record({"plainProvider": params.get("modelProvider") is None,
                    "customProvider": params.get("modelProvider") == "custom-gateway",
                    "customUrl": isinstance(custom, dict) and custom.get("base_url") == GATEWAY,
                    "noCustomConfig": not isinstance(custom, dict)})
            # Stop before native thread creation, model listing, or any turn.
            reply(request_id, error={"code": -32603, "message": "synthetic thread stop"})


if __name__ == "__main__":
    raise SystemExit(main())
