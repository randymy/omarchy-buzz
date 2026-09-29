#!/usr/bin/env python3
"""Account-free ACP check of the compiled Claude subscription-policy proposal.

Run as non-root inside a separate loopback-only network namespace. The probe
sends initialize and control requests that policy mode must reject. It never
starts login, creates a session, or submits a prompt.
"""

import argparse
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import tempfile
import time

from vendor_adapter_discovery import (
    MAX_CAPTURE, ProbeFailure, VERSION, capture_bounded, clean_child_env,
    installed_version, network_isolated,
)


EXPECTED_ADAPTER = "0.82.0"
EXPECTED_NATIVE = "0.3.280"
POLICY_REASON = "claude_subscription_route_unavailable"
POLICY_MESSAGE = "Claude subscription route is unavailable."
TOTAL_TIMEOUT = 45
SESSION_ID = "00000000-0000-0000-0000-000000000001"
REQUESTS = (
    (2, "authenticate", {"methodId": "api-key"}, "apiAuthRejected"),
    (3, "authenticate", {"methodId": "gateway", "_meta": {
        "gateway": {"baseUrl": "https://synthetic.invalid", "headers": {}}
    }}, "gatewayAuthRejected"),
    (4, "providers/set", {"providerId": "synthetic", "apiType": "anthropic",
                          "baseUrl": "https://synthetic.invalid"}, "providerSetRejected"),
    (5, "providers/disable", {"providerId": "synthetic"}, "providerDisableRejected"),
    (6, "session/load", {"sessionId": SESSION_ID, "cwd": "/tmp", "mcpServers": []},
     "loadRejected"),
    (7, "session/resume", {"sessionId": SESSION_ID, "cwd": "/tmp", "mcpServers": []},
     "resumeRejected"),
    (8, "session/fork", {"sessionId": SESSION_ID, "cwd": "/tmp", "mcpServers": []},
     "forkRejected"),
    (9, "_session/steering", {"sessionId": SESSION_ID,
                              "prompt": [{"type": "text", "text": "synthetic"}]},
     "steeringRejected"),
)


def require_response(message, expected_id):
    if (not isinstance(message, dict) or message.get("jsonrpc") != "2.0"
            or type(message.get("id")) is not int or message["id"] != expected_id):
        raise ProbeFailure("acp_response_invalid")
    if expected_id == 1:
        result = message.get("result")
        if "error" in message or not isinstance(result, dict):
            raise ProbeFailure("acp_initialize_rejected")
        methods = result.get("authMethods")
        if not isinstance(methods, list) or len(methods) != 1:
            raise ProbeFailure("acp_auth_methods_unexpected")
        method = methods[0]
        if (not isinstance(method, dict) or method.get("id") != "claude-ai-login"
                or method.get("type") != "terminal"
                or method.get("args") != ["--require-claude-subscription", "--cli",
                                              "auth", "login", "--claudeai"]):
            raise ProbeFailure("acp_auth_methods_unexpected")
        terminal = method.get("_meta", {}).get("terminal-auth") if isinstance(
            method.get("_meta"), dict) else None
        if (not isinstance(terminal, dict) or not isinstance(terminal.get("args"), list)
                or terminal["args"][-3:] != ["auth", "login", "--claudeai"]
                or "--require-claude-subscription" not in terminal["args"]
                or "--cli" not in terminal["args"]):
            raise ProbeFailure("acp_terminal_auth_invalid")
        capabilities = result.get("agentCapabilities")
        if not isinstance(capabilities, dict):
            raise ProbeFailure("acp_capabilities_invalid")
        sessions = capabilities.get("sessionCapabilities")
        mcp = capabilities.get("mcpCapabilities")
        meta = result.get("_meta")
        if (not isinstance(sessions, dict) or not isinstance(mcp, dict)
                or "providers" in capabilities or "loadSession" in capabilities
                or any(key in sessions for key in
                       ("resume", "fork", "additionalDirectories", "subagents"))
                or mcp.get("http") is not False or mcp.get("sse") is not False
                or (isinstance(meta, dict) and "steering" in meta)):
            raise ProbeFailure("acp_denied_capability_advertised")
        return
    error = message.get("error")
    if "result" in message or not isinstance(error, dict):
        raise ProbeFailure("acp_policy_request_not_rejected")
    if (type(error.get("code")) is not int or error["code"] != -32000
            or error.get("message") != POLICY_MESSAGE
            or error.get("data") != {"reason": POLICY_REASON}):
        raise ProbeFailure("acp_policy_rejection_mismatch")


def _send(process, request_id, method, params):
    payload = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
    try:
        process.stdin.write((json.dumps(payload, separators=(",", ":")) + "\n").encode())
        process.stdin.flush()
    except (BrokenPipeError, OSError):
        raise ProbeFailure("acp_stdin_closed") from None


def run_probe(node, adapter, deadline, results):
    with tempfile.TemporaryDirectory(prefix="claude-policy-adapter-") as temporary:
        home = Path(temporary)
        env = clean_child_env(home, node)
        config = home / "claude"
        config.mkdir(mode=0o700)
        env["CLAUDE_CONFIG_DIR"] = str(config)
        try:
            process = subprocess.Popen(
                [str(node), str(adapter), "--require-claude-subscription"],
                cwd=home, env=env, stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                start_new_session=True,
            )
        except OSError:
            raise ProbeFailure("acp_spawn_failed") from None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ, "stdout")
        selector.register(process.stderr, selectors.EVENT_READ, "stderr")
        pending = bytearray()
        captured = 0
        expected_id = 1
        try:
            _send(process, 1, "initialize", {
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {"readTextFile": False, "writeTextFile": False},
                    "terminal": False,
                    "auth": {"terminal": True, "_meta": {"gateway": True}},
                    "_meta": {"terminal-auth": True},
                },
            })
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ProbeFailure("acp_timeout")
                if process.poll() is not None and not selector.get_map():
                    raise ProbeFailure("acp_exited_early")
                for key, _ in selector.select(min(remaining, 0.2)):
                    try:
                        chunk = os.read(key.fileobj.fileno(), 4096)
                    except OSError:
                        raise ProbeFailure("acp_read_failed") from None
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    captured += len(chunk)
                    if captured > MAX_CAPTURE:
                        raise ProbeFailure("acp_output_limit")
                    if key.data != "stdout":
                        continue
                    pending.extend(chunk)
                    while b"\n" in pending:
                        line, _, rest = pending.partition(b"\n")
                        pending = bytearray(rest)
                        try:
                            message = json.loads(line)
                        except (UnicodeError, ValueError):
                            raise ProbeFailure("acp_json_invalid") from None
                        if not isinstance(message, dict) or "id" not in message:
                            continue
                        require_response(message, expected_id)
                        if expected_id == 1:
                            results["initializeConformant"] = True
                        else:
                            results[REQUESTS[expected_id - 2][3]] = True
                        if expected_id == 1 + len(REQUESTS):
                            return
                        request_id, method, params, _ = REQUESTS[expected_id - 1]
                        expected_id = request_id
                        _send(process, request_id, method, params)
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
            selector.close()
            process.stdin.close()
            process.stdout.close()
            process.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter-source", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    summary = {
        "successful": False, "networkIsolated": False,
        "realAccountUsed": False, "modelTurnRequested": False,
        "adapterVersion": None, "nativeVersion": None, "nodeVersion": None,
        "initializeConformant": False,
        **{label: False for _, _, _, label in REQUESTS},
        "failures": [],
    }
    deadline = time.monotonic() + TOTAL_TIMEOUT
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not args.adapter_source.is_absolute() or not args.node.is_absolute():
            raise ProbeFailure("input_path_not_absolute")
        source = args.adapter_source.resolve(strict=True)
        node = args.node.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        package = json.loads((source / "package.json").read_text())
        if (not isinstance(package, dict)
                or package.get("name") != "@agentclientprotocol/claude-agent-acp"
                or package.get("version") != EXPECTED_ADAPTER):
            raise ProbeFailure("adapter_identity_mismatch")
        summary["adapterVersion"] = EXPECTED_ADAPTER
        if installed_version(source, "@anthropic-ai/claude-agent-sdk") != EXPECTED_NATIVE:
            raise ProbeFailure("native_version_mismatch")
        summary["nativeVersion"] = EXPECTED_NATIVE
        adapter = (source / "dist" / "index.js").resolve(strict=True)
        if not adapter.is_file() or not adapter.is_relative_to(source):
            raise ProbeFailure("adapter_entry_invalid")
        with tempfile.TemporaryDirectory(prefix="claude-policy-node-version-") as temporary:
            home = Path(temporary)
            raw = capture_bounded([str(node), "--version"], clean_child_env(home, node), home,
                                  min(5, max(0.01, deadline - time.monotonic())))
        version = raw.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(version) or int(version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = version
        run_probe(node, adapter, deadline, summary)
        summary["successful"] = True
    except ProbeFailure as error:
        summary["failures"].append(error.category)
    except (OSError, ValueError):
        summary["failures"].append("probe_setup_failed")
    encoded = json.dumps(summary, sort_keys=True) + "\n"
    args.output.write_text(encoded)
    print(encoded, end="")
    return 0 if summary["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
