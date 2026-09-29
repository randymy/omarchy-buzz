#!/usr/bin/env python3
"""Characterize pinned Claude native auth status with disposable credentials only.

Run as non-root inside a separate loopback-only network namespace. This calls
only `claude-agent-acp --cli auth status --json`; it never logs in, starts a
session, or sends a prompt. The result is characterization, not a policy or
billing guarantee. Raw CLI output, stderr, and paths are never reported.
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
    ProbeFailure, VERSION, adapter_executable, capture_bounded,
    clean_child_env, installed_version, network_isolated,
)


ADAPTER_VERSION = "0.82.0"
SDK_VERSION = "0.3.280"
TOTAL_TIMEOUT = 35
CASE_TIMEOUT = 8
MAX_OUTPUT = 16 * 1024
SYNTHETIC_KEY = "sk-ant-api03-" + "A" * 80
CASES = ("empty", "apiKey", "apiKeyForcedClaudeAi")
AUTH_METHODS = {"none", "claude.ai", "api_key"}
PROVIDERS = {"firstParty", "bedrock", "vertex", "gateway"}
KEY_SOURCES = {
    "ANTHROPIC_API_KEY", "apiKeyHelper", "/login managed key",
    "none", "user", "project", "org", "temporary", "oauth",
}


def capture_status(argv, env, cwd, timeout):
    """Return bounded stdout and an exit class; discard bounded stderr."""
    try:
        process = subprocess.Popen(
            argv, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError:
        raise ProbeFailure("native_spawn_failed") from None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    output = bytearray()
    seen = 0
    deadline = time.monotonic() + timeout
    try:
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("native_timeout")
            for key, _ in selector.select(min(remaining, 0.2)):
                chunk = os.read(key.fileobj.fileno(), 4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                seen += len(chunk)
                if seen > MAX_OUTPUT:
                    raise ProbeFailure("native_output_limit")
                if key.data == "stdout":
                    output.extend(chunk)
        try:
            process.wait(timeout=max(0, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            raise ProbeFailure("native_timeout") from None
        if process.returncode == 0:
            exit_class = "zero"
        elif process.returncode == 1:
            exit_class = "one"
        else:
            raise ProbeFailure("native_exit_unrecognized")
        return bytes(output), exit_class
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
        process.stdout.close()
        process.stderr.close()


def classify_status(raw, exit_class):
    try:
        payload = json.loads(raw)
    except (UnicodeError, ValueError):
        raise ProbeFailure("status_json_invalid") from None
    if not isinstance(payload, dict) or type(payload.get("loggedIn")) is not bool:
        raise ProbeFailure("status_shape_invalid")
    result = {"loggedIn": payload["loggedIn"], "exitClass": exit_class}
    for field, allowed in (
        ("authMethod", AUTH_METHODS),
        ("apiProvider", PROVIDERS),
        ("apiKeySource", KEY_SOURCES),
    ):
        value = payload.get(field)
        if value is not None and (not isinstance(value, str) or value not in allowed):
            raise ProbeFailure("status_" + field + "_unrecognized")
        result[field] = value
    return result


def run_case(node, adapter, case, deadline):
    if case not in CASES:
        raise ProbeFailure("case_invalid")
    with tempfile.TemporaryDirectory(prefix="claude-auth-status-") as temporary:
        home = Path(temporary)
        env = clean_child_env(home, node)
        # The adapter's `--cli` delegates to its pinned bundled native CLI.
        if case != "empty":
            env["ANTHROPIC_API_KEY"] = SYNTHETIC_KEY
        if case == "apiKeyForcedClaudeAi":
            claude_dir = home / ".claude"
            claude_dir.mkdir(mode=0o700)
            (claude_dir / "settings.json").write_text(
                '{"forceLoginMethod":"claudeai"}\n'
            )
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ProbeFailure("probe_timeout")
        raw, exit_class = capture_status(
            [str(node), str(adapter), "--cli", "auth", "status", "--json"],
            env, home, min(CASE_TIMEOUT, remaining),
        )
    return classify_status(raw, exit_class)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packages", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    summary = {
        "successful": False,
        "characterizationOnly": True,
        "realAccountUsed": False,
        "modelTurnRequested": False,
        "networkIsolated": False,
        "adapterVersion": None,
        "sdkVersion": None,
        "nodeVersion": None,
        "cases": {},
        "failures": [],
    }
    active_case = None
    deadline = time.monotonic() + TOTAL_TIMEOUT
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not args.packages.is_absolute() or not args.node.is_absolute():
            raise ProbeFailure("input_path_not_absolute")
        packages = args.packages.resolve(strict=True)
        node = args.node.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        if installed_version(packages, "@agentclientprotocol/claude-agent-acp") != ADAPTER_VERSION:
            raise ProbeFailure("adapter_version_mismatch")
        summary["adapterVersion"] = ADAPTER_VERSION
        if installed_version(packages, "@anthropic-ai/claude-agent-sdk") != SDK_VERSION:
            raise ProbeFailure("sdk_version_mismatch")
        summary["sdkVersion"] = SDK_VERSION
        adapter = adapter_executable(packages, "claude-agent-acp")
        with tempfile.TemporaryDirectory(prefix="claude-node-version-") as temporary:
            home = Path(temporary)
            raw = capture_bounded(
                [str(node), "--version"], clean_child_env(home, node), home,
                min(5, max(0.01, deadline - time.monotonic())),
            )
        version = raw.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(version) or int(version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = version
        for active_case in CASES:
            summary["cases"][active_case] = run_case(node, adapter, active_case, deadline)
        active_case = None
        summary["successful"] = True
    except ProbeFailure as error:
        summary["failures"].append(error.category)
        if active_case:
            summary["failedCase"] = active_case
    except (OSError, ValueError):
        summary["failures"].append("probe_setup_failed")
        if active_case:
            summary["failedCase"] = active_case
    encoded = json.dumps(summary, sort_keys=True) + "\n"
    args.output.write_text(encoded)
    print(encoded, end="")
    return 0 if summary["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
