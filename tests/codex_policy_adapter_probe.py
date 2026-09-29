#!/usr/bin/env python3
"""Account-free ACP check of the built Codex subscription-policy adapter.

Run only as non-root in a separate loopback-only network namespace. This sends
initialize and denied control requests. It never authenticates, opens a
browser, creates/loads a session, sends a prompt, or uses a real account.
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


EXPECTED_ADAPTER = "2.0.0"
EXPECTED_NATIVE = "0.158.0"
POLICY_ERROR = "subscription_policy_denied"
TOTAL_TIMEOUT = 45
REQUESTS = (
    (2, "authenticate", {"methodId": "api-key"}, "apiAuthRejected"),
    (3, "providers/set", {"providerId": "openai", "apiType": "openai",
                           "baseUrl": "https://synthetic.invalid"}, "providerSetRejected"),
    (4, "session/load", {"sessionId": "00000000-0000-0000-0000-000000000001",
                          "cwd": "/tmp", "mcpServers": []}, "loadRejected"),
    (5, "session/resume", {"sessionId": "00000000-0000-0000-0000-000000000001",
                            "cwd": "/tmp", "mcpServers": []}, "resumeRejected"),
    (6, "session/fork", {"sessionId": "00000000-0000-0000-0000-000000000001",
                          "cwd": "/tmp", "mcpServers": []}, "forkRejected"),
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
        if (not isinstance(methods, list) or not methods or not all(
            isinstance(method, dict) and method.get("id") in
            ("chat-gpt", "chat-gpt-device-code") for method in methods
        )):
            raise ProbeFailure("acp_auth_methods_unexpected")
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
                or mcp.get("http") is not False
                or (isinstance(meta, dict) and "steering" in meta)):
            raise ProbeFailure("acp_denied_capability_advertised")
        return
    error = message.get("error")
    if "result" in message or not isinstance(error, dict):
        raise ProbeFailure("acp_policy_request_not_rejected")
    code, detail, data = error.get("code"), error.get("message"), error.get("data")
    direct = type(code) is int and code == -32600 and detail == POLICY_ERROR and data is None
    wrapped = (type(code) is int and code == -32603 and detail == "Internal error"
               and isinstance(data, dict) and data == {"details": POLICY_ERROR})
    if not (direct or wrapped):
        raise ProbeFailure("acp_policy_rejection_mismatch")


def _send(process, request_id, method, params):
    payload = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
    try:
        process.stdin.write((json.dumps(payload, separators=(",", ":")) + "\n").encode())
        process.stdin.flush()
    except (BrokenPipeError, OSError):
        raise ProbeFailure("acp_stdin_closed") from None


def run_probe(node, adapter, deadline, results):
    with tempfile.TemporaryDirectory(prefix="codex-policy-adapter-") as temporary:
        home = Path(temporary)
        codex_home = home / "codex"
        codex_home.mkdir(mode=0o700)
        env = clean_child_env(home, node)
        env["CODEX_HOME"] = str(codex_home)
        try:
            process = subprocess.Popen(
                [str(node), str(adapter), "--require-chatgpt-subscription"],
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
        if not isinstance(package, dict) or package.get("name") != "@agentclientprotocol/codex-acp" \
                or package.get("version") != EXPECTED_ADAPTER:
            raise ProbeFailure("adapter_identity_mismatch")
        summary["adapterVersion"] = EXPECTED_ADAPTER
        if installed_version(source, "@openai/codex") != EXPECTED_NATIVE:
            raise ProbeFailure("native_version_mismatch")
        summary["nativeVersion"] = EXPECTED_NATIVE
        adapter = (source / "dist" / "index.js").resolve(strict=True)
        if not adapter.is_file() or not adapter.is_relative_to(source):
            raise ProbeFailure("adapter_entry_invalid")
        with tempfile.TemporaryDirectory(prefix="codex-policy-node-version-") as temporary:
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
