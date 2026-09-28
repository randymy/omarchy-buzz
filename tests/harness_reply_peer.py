#!/usr/bin/env python3
"""Text-only ACP peer for harness-owned reply tests; no model, tools or network."""
import json
import re
import sys


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def answer(request_id, result):
    send({"jsonrpc": "2.0", "id": request_id, "result": result})


session_counter = 0
active_sessions = set()
pending_permission = None
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    request_id = request.get("id")
    if pending_permission is not None and method is None and request_id == "fixture-permission":
        prompt_id, session, token = pending_permission
        pending_permission = None
        outcome = (request.get("result") or {}).get("outcome") or {}
        if outcome.get("outcome") != "selected" or outcome.get("optionId") != "deny":
            send({"jsonrpc": "2.0", "id": prompt_id,
                  "error": {"code": -32603, "message": "fixture permission was not denied"}})
            continue
        send({"jsonrpc": "2.0", "method": "session/update",
              "params": {"sessionId": session,
                         "update": {"sessionUpdate": "agent_message_chunk",
                                    "content": {"type": "text", "text": f"AE-ACK:{token}"}}}})
        answer(prompt_id, {"stopReason": "end_turn"})
        continue
    if method == "initialize":
        answer(request_id, {"protocolVersion": 2, "agentCapabilities": {}})
    elif method == "session/new":
        session_counter += 1
        session = f"synthetic-reply-{session_counter}"
        active_sessions.add(session)
        answer(request_id, {"sessionId": session,
                            "modes": {"availableModes": [{"id": "read-only"}]},
                            "configOptions": [{"id": "mode", "currentValue": "default"}]})
    elif method == "session/set_config_option":
        params = request.get("params") or {}
        if params.get("sessionId") not in active_sessions or params.get("configId") != "mode" or params.get("value") != "read-only":
            send({"jsonrpc": "2.0", "id": request_id,
                  "error": {"code": -32602, "message": "unsupported fixture mode"}})
        else:
            answer(request_id, {"configOptions": [{"id": "mode", "currentValue": "read-only"}]})
    elif method == "session/prompt":
        params = request.get("params") or {}
        session = params.get("sessionId")
        if session not in active_sessions:
            send({"jsonrpc": "2.0", "id": request_id,
                  "error": {"code": -32602, "message": "unknown fixture session"}})
            continue
        prompt = "\n".join(part.get("text", "") for part in params.get("prompt", [])
                           if part.get("type") == "text" and isinstance(part.get("text"), str))
        tokens = re.findall(r"\bAE-ID:([A-Za-z0-9._:-]+)\b", prompt)
        if len(tokens) != 1:
            send({"jsonrpc": "2.0", "id": request_id,
                  "error": {"code": -32602, "message": "expected one fixture token"}})
            continue
        send({"jsonrpc": "2.0", "method": "session/update",
              "params": {"sessionId": session,
                         "update": {"sessionUpdate": "agent_thought_chunk",
                                    "content": {"type": "text", "text": "THOUGHT-MUST-NOT-PUBLISH"}}}})
        pending_permission = (request_id, session, tokens[0])
        send({"jsonrpc": "2.0", "id": "fixture-permission",
              "method": "session/request_permission",
              "params": {"sessionId": session,
                         "toolCall": {"toolCallId": "fixture-tool", "title": "Fixture tool (never executed)"},
                         "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                                     {"optionId": "deny", "name": "Deny", "kind": "reject_once"}]}})
    elif method == "session/cancel":
        if request_id is not None:
            answer(request_id, {})
    elif request_id is not None:
        send({"jsonrpc": "2.0", "id": request_id,
              "error": {"code": -32601, "message": "unsupported fixture method"}})
