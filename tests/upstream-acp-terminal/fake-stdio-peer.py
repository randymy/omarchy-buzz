#!/usr/bin/env python3
"""Disposable ACP peer and interactive login process for buzz-acp black-box tests.

The same configured executable handles stdio ACP and the terminal method's
appended --login argument. It never opens a network connection or credential
store. Trace records are deliberately metadata-only.
"""

import json
import os
import sys
import time


def record(path, event, **fields):
    with open(path, "a", encoding="utf-8") as stream:
        stream.write(json.dumps({"event": event, **fields}, sort_keys=True) + "\n")
        stream.flush()


def main():
    if len(sys.argv) < 3:
        raise SystemExit(2)
    trace, case, *extra = sys.argv[1:]
    # The harness must filter caller/provider data before discovery and login.
    for key in ("BUZZ_PRIVATE_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "CODEX_CONFIG"):
        if key in os.environ:
            record(trace, "inherited_forbidden_env", key=key)
            raise SystemExit(3)

    profiles = {
        "codex_home": os.environ.get("CODEX_HOME"),
        "claude_config_dir": os.environ.get("CLAUDE_CONFIG_DIR"),
    }

    if extra:
        if extra != ["--login"]:
            record(trace, "unexpected_args", args=extra)
            raise SystemExit(4)
        record(
            trace,
            "login",
            tty=all(os.isatty(fd) for fd in (0, 1, 2)),
            foreground=os.tcgetpgrp(0) == os.getpgrp(),
            marker=os.environ.get("ACP_INTERACTIVE_LOGIN"),
            **profiles,
        )
        if case == "nonzero":
            raise SystemExit(7)
        if case == "input":
            print("synthetic-login-input", flush=True)
            answer = input()
            record(trace, "input", correct=answer == "synthetic-answer")
            raise SystemExit(0 if answer == "synthetic-answer" else 8)
        if case == "cancel":
            while True:
                time.sleep(0.05)
        raise SystemExit(0)

    record(trace, "spawn", **profiles)
    for line in sys.stdin:
        request = json.loads(line)
        method = request.get("method")
        if method == "initialize":
            terminal = request.get("params", {}).get("clientCapabilities", {}).get("auth", {}).get("terminal")
            record(trace, "initialize", terminal=terminal)
            if case == "reconnect-fail" and sum(1 for row in read_trace(trace) if row["event"] == "initialize") > 1:
                response = {"jsonrpc": "2.0", "id": request["id"], "error": {"code": -32000, "message": "synthetic reconnect failure"}}
            else:
                descriptor = {"id": "login", "type": "terminal", "args": ["--login"], "env": {"ACP_INTERACTIVE_LOGIN": "1"}}
                if case == "command":
                    descriptor["command"] = "/bin/sh"
                elif case == "unsafe-env":
                    descriptor["env"] = {"PATH": "/tmp"}
                elif case == "agent":
                    descriptor = {"id": "login"}  # ACP defaults absent type to agent.
                elif case == "unknown-type":
                    descriptor["type"] = "future"
                methods = [descriptor] if terminal or case in ("force-terminal", "agent", "unknown-type") else []
                if case == "duplicate" and methods:
                    methods.append(descriptor.copy())
                response = {"jsonrpc": "2.0", "id": request["id"], "result": {"protocolVersion": 1, "authMethods": methods}}
        elif method == "authenticate":
            record(trace, "authenticate", method_id=request.get("params", {}).get("methodId"))
            response = {"jsonrpc": "2.0", "id": request["id"], "result": {}}
        else:
            record(trace, "unexpected_method", method=method)
            response = {"jsonrpc": "2.0", "id": request["id"], "error": {"code": -32601, "message": "not implemented"}}
        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()


def read_trace(path):
    with open(path, encoding="utf-8") as stream:
        return [json.loads(line) for line in stream if line.strip()]


if __name__ == "__main__":
    main()
