#!/usr/bin/env python3
"""Account-free characterization of codex-acp's ACP provider mutation path.

Run non-root in a loopback-only network namespace. The adapter is real; its
native app-server child is scripted and rejects thread creation. No login,
browser, prompt, provider request, or model turn is performed.
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
    MAX_CAPTURE, ProbeFailure, VERSION, adapter_executable, capture_bounded,
    clean_child_env, installed_version, network_isolated,
)

ADAPTER_VERSION = "2.0.0"
NATIVE_VERSION = "0.158.0"
GATEWAY = "https://routing-probe.invalid/v1"
TOTAL_TIMEOUT = 45
LINE_LIMIT = 16 * 1024


def _send(process, request_id, method, params):
    value = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
    data = (json.dumps(value, separators=(",", ":")) + "\n").encode()
    if len(data) > LINE_LIMIT:
        raise ProbeFailure("acp_request_too_large")
    try:
        process.stdin.write(data)
        process.stdin.flush()
    except (BrokenPipeError, OSError):
        raise ProbeFailure("acp_stdin_closed") from None


def _response(message, expected_id):
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0" or type(message.get("id")) is not int or message["id"] != expected_id:
        raise ProbeFailure("acp_response_invalid")
    if expected_id == 1:
        result = message.get("result")
        info = result.get("agentInfo") if isinstance(result, dict) else None
        capabilities = result.get("agentCapabilities") if isinstance(result, dict) else None
        if not isinstance(info, dict) or "error" in message or info.get("version") != ADAPTER_VERSION:
            raise ProbeFailure("acp_initialize_rejected")
        if not isinstance(capabilities, dict) or not isinstance(capabilities.get("providers"), dict):
            raise ProbeFailure("acp_provider_capability_missing")
    elif expected_id in (2, 4):
        # The scripted native peer refuses thread/start before any model task.
        if not isinstance(message.get("error"), dict) or "result" in message:
            raise ProbeFailure("synthetic_thread_not_stopped")
    elif expected_id == 3:
        if not isinstance(message.get("result"), dict) or "error" in message:
            raise ProbeFailure("provider_mutation_rejected")
    else:
        raise ProbeFailure("acp_unexpected_response")


def _captures(state):
    try:
        raw = state.read_bytes()
        if len(raw) > 2048:
            raise ProbeFailure("native_capture_limit")
        captures = [json.loads(line) for line in raw.splitlines()]
    except (OSError, ValueError, UnicodeError):
        raise ProbeFailure("native_capture_invalid") from None
    keys = {"plainProvider", "customProvider", "customUrl", "noCustomConfig"}
    if len(captures) != 2 or any(not isinstance(item, dict) or set(item) != keys or any(type(v) is not bool for v in item.values()) for item in captures):
        raise ProbeFailure("native_capture_invalid")
    before, after = captures
    if before != {"plainProvider": True, "customProvider": False, "customUrl": False, "noCustomConfig": True}:
        raise ProbeFailure("plain_route_mismatch")
    if after != {"plainProvider": False, "customProvider": True, "customUrl": True, "noCustomConfig": False}:
        raise ProbeFailure("gateway_route_mismatch")


def run_probe(node, adapter, deadline):
    with tempfile.TemporaryDirectory(prefix="codex-routing-probe-") as temporary:
        home = Path(temporary)
        codex_home = home / "codex"
        workspace = home / "workspace"
        codex_home.mkdir(mode=0o700)
        workspace.mkdir(mode=0o700)
        (codex_home / "config.toml").write_text('forced_login_method = "chatgpt"\n')
        fixture = Path(__file__).parent / "fixtures" / "codex_routing_native_peer.py"
        native_peer = home / "native-peer"
        data = fixture.read_bytes()
        if len(data) > LINE_LIMIT:
            raise ProbeFailure("native_fixture_invalid")
        native_peer.write_bytes(data)
        native_peer.chmod(0o700)
        state = home / "captures.jsonl"
        env = clean_child_env(home, node)
        env.update({"CODEX_HOME": str(codex_home), "CODEX_PATH": str(native_peer),
                    "CODEX_ROUTING_STATE": str(state)})
        try:
            process = subprocess.Popen([str(node), str(adapter)], cwd=workspace, env=env,
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, start_new_session=True)
        except OSError:
            raise ProbeFailure("adapter_spawn_failed") from None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ, "stdout")
        selector.register(process.stderr, selectors.EVENT_READ, "stderr")
        pending = bytearray()
        captured = 0
        expected_id = 1
        try:
            _send(process, 1, "initialize", {"protocolVersion": 1,
                "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False}})
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ProbeFailure("probe_timeout")
                if process.poll() is not None and not selector.get_map():
                    raise ProbeFailure("adapter_exited_early")
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
                    if len(pending) > LINE_LIMIT:
                        raise ProbeFailure("acp_line_limit")
                    while b"\n" in pending:
                        line, _, rest = pending.partition(b"\n")
                        pending = bytearray(rest)
                        try:
                            message = json.loads(line)
                        except (UnicodeError, ValueError):
                            raise ProbeFailure("acp_json_invalid") from None
                        if not isinstance(message, dict):
                            raise ProbeFailure("acp_json_invalid")
                        if "id" not in message:
                            if "method" not in message or "result" in message or "error" in message:
                                raise ProbeFailure("acp_notification_invalid")
                            continue
                        _response(message, expected_id)
                        if expected_id == 1:
                            expected_id = 2
                            _send(process, 2, "session/new", {"cwd": str(workspace), "mcpServers": []})
                        elif expected_id == 2:
                            expected_id = 3
                            _send(process, 3, "providers/set", {"providerId": "openai", "apiType": "openai",
                                                               "baseUrl": GATEWAY, "headers": {}})
                        elif expected_id == 3:
                            expected_id = 4
                            _send(process, 4, "session/new", {"cwd": str(workspace), "mcpServers": []})
                        else:
                            _captures(state)
                            return
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
    parser.add_argument("--packages", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    summary = {"successful": False, "networkIsolated": False,
               "actualAdapter": False, "nativePeer": "scripted", "characterizationOnly": True,
               "realAccountUsed": False, "adapterVersion": None,
               "nativePackageVersion": None, "nodeVersion": None,
               "plainThreadStartObserved": False, "providerMutationAccepted": False,
               "gatewayThreadStartObserved": False, "modelTurnRequested": False, "failures": []}
    deadline = time.monotonic() + TOTAL_TIMEOUT
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not args.packages.is_absolute() or not args.node.is_absolute():
            raise ProbeFailure("input_path_not_absolute")
        packages = args.packages.resolve(strict=True)
        node = args.node.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        summary["adapterVersion"] = installed_version(packages, "@agentclientprotocol/codex-acp")
        summary["nativePackageVersion"] = installed_version(packages, "@openai/codex")
        if summary["adapterVersion"] != ADAPTER_VERSION or summary["nativePackageVersion"] != NATIVE_VERSION:
            raise ProbeFailure("package_version_mismatch")
        adapter = adapter_executable(packages, "codex-acp")
        with tempfile.TemporaryDirectory(prefix="codex-routing-node-version-") as temporary:
            home = Path(temporary)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), "--version"], clean_child_env(home, node), home,
                                  min(5, remaining))
        version = raw.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(version) or int(version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = version
        run_probe(node, adapter, deadline)
        summary.update({"actualAdapter": True, "plainThreadStartObserved": True, "providerMutationAccepted": True,
                        "gatewayThreadStartObserved": True, "successful": True})
    except ProbeFailure as error:
        summary["failures"].append(error.category)
    except (OSError, ValueError):
        summary["failures"].append("probe_setup_failed")
    args.output.write_text(json.dumps(summary, sort_keys=True) + "\n")
    print(json.dumps(summary, sort_keys=True))
    return 0 if summary["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
