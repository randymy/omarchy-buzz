#!/usr/bin/env python3
"""Account-free Codex app-server API-login rejection probe.

Run only as a non-root user in a separate, loopback-only network namespace.
This sends initialize and one synthetic API-key login request; it never starts
a thread, sends a prompt, opens a browser, or uses a real account.
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
    MAX_CAPTURE,
    ProbeFailure,
    VERSION,
    capture_bounded,
    clean_child_env,
    installed_version,
    network_isolated,
)


EXPECTED_NATIVE = "0.158.0"
EXPECTED_ERROR_CODE = -32600
EXPECTED_ERROR_MESSAGE = "API key login is disabled. Use ChatGPT login instead."
TOTAL_TIMEOUT = 45
SYNTHETIC_KEY = "sk-test-key"


def native_entry(packages):
    package = packages / "node_modules" / "@openai" / "codex"
    entry = package / "bin" / "codex.js"
    try:
        package.resolve(strict=True).relative_to(packages.resolve(strict=True))
        resolved = entry.resolve(strict=True)
        resolved.relative_to(package.resolve(strict=True))
    except (OSError, ValueError):
        raise ProbeFailure("native_entry_outside_package") from None
    if not resolved.is_file():
        raise ProbeFailure("native_entry_unavailable")
    return resolved


def _write_rpc(process, payload):
    encoded = (json.dumps(payload, separators=(",", ":")) + "\n").encode("utf-8")
    try:
        process.stdin.write(encoded)
        process.stdin.flush()
    except (BrokenPipeError, OSError):
        raise ProbeFailure("native_stdin_closed") from None


def _require_response(message, expected_id):
    if not isinstance(message, dict) or type(message.get("id")) is not int or message.get("id") != expected_id:
        raise ProbeFailure("native_response_invalid")
    if expected_id == 1:
        if not isinstance(message.get("result"), dict) or "error" in message:
            raise ProbeFailure("initialize_rejected")
        return
    error = message.get("error")
    if not isinstance(error, dict) or "result" in message:
        raise ProbeFailure("api_login_not_rejected")
    if error.get("code") != EXPECTED_ERROR_CODE or error.get("message") != EXPECTED_ERROR_MESSAGE:
        raise ProbeFailure("api_login_rejection_mismatch")


def run_native_probe(node, entry, deadline):
    with tempfile.TemporaryDirectory(prefix="codex-subscription-probe-") as temporary:
        home = Path(temporary)
        codex_home = home / "codex"
        codex_home.mkdir(mode=0o700)
        (codex_home / "config.toml").write_text('forced_login_method = "chatgpt"\n')
        env = clean_child_env(home, node)
        env["CODEX_HOME"] = str(codex_home)
        argv = [str(node), str(entry), "-c", 'forced_login_method="chatgpt"', "app-server"]
        try:
            process = subprocess.Popen(
                argv, cwd=home, env=env, stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                start_new_session=True,
            )
        except OSError:
            raise ProbeFailure("native_spawn_failed") from None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ, "stdout")
        selector.register(process.stderr, selectors.EVENT_READ, "stderr")
        stdout_pending = bytearray()
        captured_bytes = 0
        expected_id = 1
        try:
            _write_rpc(process, {
                "id": 1, "method": "initialize",
                "params": {
                    "clientInfo": {"name": "subscription-probe", "version": "1.0.0"},
                    "capabilities": {"experimentalApi": False},
                },
            })
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ProbeFailure("native_timeout")
                if process.poll() is not None and not selector.get_map():
                    raise ProbeFailure("native_exited_before_rejection")
                for key, _ in selector.select(min(remaining, 0.2)):
                    try:
                        chunk = os.read(key.fileobj.fileno(), 4096)
                    except OSError:
                        raise ProbeFailure("native_read_failed") from None
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    captured_bytes += len(chunk)
                    if captured_bytes > MAX_CAPTURE:
                        raise ProbeFailure("native_output_limit")
                    if key.data != "stdout":
                        continue
                    stdout_pending.extend(chunk)
                    while b"\n" in stdout_pending:
                        line, _, rest = stdout_pending.partition(b"\n")
                        stdout_pending = bytearray(rest)
                        try:
                            message = json.loads(line)
                        except (UnicodeError, ValueError):
                            raise ProbeFailure("native_json_invalid") from None
                        if not isinstance(message, dict) or "id" not in message:
                            continue  # Notifications are allowed, never recorded.
                        _require_response(message, expected_id)
                        if expected_id == 2:
                            return
                        expected_id = 2
                        _write_rpc(process, {"method": "initialized"})
                        _write_rpc(process, {
                            "id": 2, "method": "account/login/start",
                            "params": {"type": "apiKey", "apiKey": SYNTHETIC_KEY},
                        })
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
        "successful": False,
        "networkIsolated": False,
        "nativePackageVersion": None,
        "nodeVersion": None,
        "nativeBinaryVersion": None,
        "startupPolicy": "chatgpt",
        "apiLoginRejected": False,
        "rejectionCode": None,
        "failures": [],
    }
    deadline = time.monotonic() + TOTAL_TIMEOUT
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not args.packages.is_absolute() or not args.node.is_absolute():
            raise ProbeFailure("input_path_not_absolute")
        packages = args.packages.resolve(strict=True)
        node = args.node.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        version = installed_version(packages, "@openai/codex")
        summary["nativePackageVersion"] = version
        if version != EXPECTED_NATIVE:
            raise ProbeFailure("native_package_version_mismatch")
        entry = native_entry(packages)
        with tempfile.TemporaryDirectory(prefix="codex-node-version-") as temporary:
            home = Path(temporary)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), "--version"], clean_child_env(home, node), home, min(5, remaining))
        node_version = raw.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(node_version) or int(node_version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = node_version
        with tempfile.TemporaryDirectory(prefix="codex-native-version-") as temporary:
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
        run_native_probe(node, entry, deadline)
        summary["apiLoginRejected"] = True
        summary["rejectionCode"] = EXPECTED_ERROR_CODE
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
