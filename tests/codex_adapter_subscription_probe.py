#!/usr/bin/env python3
"""Offline ACP api-key rejection through pinned codex-acp and native Codex.

Run only as a non-root user in a loopback-only network namespace. This sends
ACP initialize/authenticate only; no session, prompt, browser or real account.
The result proves this adapter path rejected this synthetic key, not payer mode.
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

from codex_subscription_probe import EXPECTED_NATIVE, EXPECTED_ERROR_MESSAGE, native_entry
from vendor_adapter_discovery import (
    MAX_CAPTURE, ProbeFailure, VERSION, adapter_executable, capture_bounded,
    clean_child_env, installed_version, network_isolated,
)

EXPECTED_ADAPTER = "2.0.0"
SYNTHETIC_KEY = "sk-test-key"
TOTAL_TIMEOUT = 45


def require_response(message, expected_id):
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0" or type(message.get("id")) is not int or message["id"] != expected_id:
        raise ProbeFailure("acp_response_invalid")
    if expected_id == 1:
        result = message.get("result")
        if "error" in message or not isinstance(result, dict):
            raise ProbeFailure("acp_initialize_rejected")
        methods = result.get("authMethods")
        if not isinstance(methods, list) or not any(
            isinstance(method, dict) and method.get("id") == "api-key" for method in methods
        ):
            raise ProbeFailure("acp_api_method_missing")
        return
    error = message.get("error")
    if "result" in message or not isinstance(error, dict):
        raise ProbeFailure("acp_api_auth_not_rejected")
    code, detail = error.get("code"), error.get("message")
    # Adapter may rewrap native invalid-request as internal-error. The exact
    # native policy sentence is essential; generic errors cannot pass.
    if type(code) is not int or code not in (-32600, -32603) or not isinstance(detail, str) or EXPECTED_ERROR_MESSAGE not in detail:
        raise ProbeFailure("acp_api_rejection_mismatch")


def _write_rpc(process, payload):
    try:
        process.stdin.write((json.dumps({"jsonrpc": "2.0", **payload}, separators=(",", ":")) + "\n").encode())
        process.stdin.flush()
    except (BrokenPipeError, OSError):
        raise ProbeFailure("acp_stdin_closed") from None


def run_adapter_probe(node, executable, deadline):
    with tempfile.TemporaryDirectory(prefix="codex-acp-subscription-probe-") as temporary:
        home = Path(temporary)
        codex_home = home / "codex"
        codex_home.mkdir(mode=0o700)
        (codex_home / "config.toml").write_text('forced_login_method = "chatgpt"\n')
        env = clean_child_env(home, node)
        env["CODEX_HOME"] = str(codex_home)
        env["OPENAI_API_KEY"] = SYNTHETIC_KEY
        # No DEFAULT_AUTH_REQUEST, CODEX_CONFIG or CODEX_PATH override. The
        # pinned adapter resolves its installed native Codex package itself.
        try:
            process = subprocess.Popen(
                [str(node), str(executable)], cwd=home, env=env,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
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
            _write_rpc(process, {
                "id": 1, "method": "initialize", "params": {
                    "protocolVersion": 1,
                    "clientCapabilities": {
                        "fs": {"readTextFile": False, "writeTextFile": False},
                        "terminal": False,
                    },
                },
            })
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ProbeFailure("acp_timeout")
                if process.poll() is not None and not selector.get_map():
                    raise ProbeFailure("acp_exited_before_rejection")
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
                            continue  # Notifications are discarded, never recorded.
                        require_response(message, expected_id)
                        if expected_id == 2:
                            return
                        expected_id = 2
                        _write_rpc(process, {"id": 2, "method": "authenticate", "params": {"methodId": "api-key"}})
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
    summary = {
        "successful": False, "networkIsolated": False,
        "adapterPackageVersion": None, "nativePackageVersion": None,
        "nativeBinaryVersion": None, "adapterBinaryVersion": None, "nodeVersion": None,
        "startupPolicy": "chatgpt", "acpApiAuthRejected": False,
        "failures": [],
    }
    deadline = time.monotonic() + TOTAL_TIMEOUT
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not all(path.is_absolute() for path in (args.packages, args.node)):
            raise ProbeFailure("input_path_not_absolute")
        packages = args.packages.resolve(strict=True)
        node = args.node.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        adapter_version = installed_version(packages, "@agentclientprotocol/codex-acp")
        adapter_root = (packages / "node_modules" / "@agentclientprotocol" / "codex-acp").resolve(strict=True)
        # Node resolves from dist/index.js; a nested copy takes precedence over
        # the root package. Validate the actual native dependency it will load.
        native_base = adapter_root if (adapter_root / "node_modules" / "@openai" / "codex" / "package.json").exists() else packages
        native_version = installed_version(native_base, "@openai/codex")
        summary["adapterPackageVersion"] = adapter_version
        summary["nativePackageVersion"] = native_version
        if adapter_version != EXPECTED_ADAPTER or native_version != EXPECTED_NATIVE:
            raise ProbeFailure("package_version_mismatch")
        executable = adapter_executable(packages, "codex-acp")
        entry = native_entry(native_base)
        with tempfile.TemporaryDirectory(prefix="codex-acp-node-version-") as temporary:
            home = Path(temporary)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), "--version"], clean_child_env(home, node), home, min(5, remaining))
        node_version = raw.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(node_version) or int(node_version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = node_version
        with tempfile.TemporaryDirectory(prefix="codex-acp-native-version-") as temporary:
            home = Path(temporary)
            env = clean_child_env(home, node)
            env["CODEX_HOME"] = str(home / "codex")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), str(entry), "--version"], env, home, min(5, remaining))
        if raw.decode("ascii", errors="ignore").strip() != "codex-cli " + EXPECTED_NATIVE:
            raise ProbeFailure("native_binary_version_mismatch")
        summary["nativeBinaryVersion"] = EXPECTED_NATIVE
        with tempfile.TemporaryDirectory(prefix="codex-acp-adapter-version-") as temporary:
            home = Path(temporary)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), str(executable), "--version"], clean_child_env(home, node), home, min(5, remaining))
        if raw.decode("ascii", errors="ignore").strip() != "@agentclientprotocol/codex-acp " + EXPECTED_ADAPTER:
            raise ProbeFailure("adapter_binary_version_mismatch")
        summary["adapterBinaryVersion"] = EXPECTED_ADAPTER
        run_adapter_probe(node, executable, deadline)
        summary["acpApiAuthRejected"] = True
        summary["successful"] = True
    except ProbeFailure as error:
        summary["failures"].append(error.category)
    except (OSError, ValueError):
        summary["failures"].append("probe_setup_failed")
    args.output.write_text(json.dumps(summary, sort_keys=True) + "\n")
    print(json.dumps(summary, sort_keys=True))
    return 0 if summary["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
