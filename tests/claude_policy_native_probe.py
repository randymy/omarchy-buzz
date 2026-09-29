#!/usr/bin/env python3
"""Check bundled Claude CLI login syntax without starting login or a model turn.

Requires a non-root isolated network namespace. Uses only --version and
`auth login --help` through the compiled adapter's native CLI forwarding path.
It deliberately does not invoke the guarded login command, which is interactive.
"""
import argparse
import json
import os
from pathlib import Path
import re
import tempfile

from vendor_adapter_discovery import (
    ProbeFailure, capture_bounded, clean_child_env, installed_version,
    network_isolated,
)

ADAPTER_VERSION = "0.82.0"
SDK_VERSION = "0.3.280"
COMMANDS = (("--version",), ("auth", "login", "--help"))


def classify(version, help_text):
    match = re.fullmatch(r"(\d+\.\d+\.\d+) \(Claude Code\)\s*", version)
    if not match:
        raise ProbeFailure("native_version_unrecognized")
    if not re.search(r"(?m)^\s*Usage:\s+.*\bauth\s+login\b", help_text):
        raise ProbeFailure("native_login_help_unrecognized")
    if not re.search(r"(?m)^\s*--claudeai(?:\s|$)", help_text):
        raise ProbeFailure("native_subscription_flag_missing")
    return {"nativeVersion": match[1], "subscriptionLoginFlagSupported": True}


def run_probe(node, adapter):
    with tempfile.TemporaryDirectory(prefix="claude-policy-native-") as temporary:
        home = Path(temporary)
        env = clean_child_env(home, node)
        config = home / "claude"
        config.mkdir(mode=0o700)
        env["CLAUDE_CONFIG_DIR"] = str(config)
        outputs = [capture_bounded([str(node), str(adapter), "--cli", *args],
                                  env, home, 15).decode("utf-8", errors="strict")
                   for args in COMMANDS]
    return classify(*outputs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adapter-source", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = {"successful": False, "networkIsolated": False,
              "realAccountUsed": False, "loginStarted": False,
              "modelTurnRequested": False, "subscriptionLoginFlagSupported": False,
              "failures": []}
    try:
        result["networkIsolated"] = network_isolated(args.outside_netns)
        if not args.node.is_absolute() or not args.adapter_source.is_absolute():
            raise ProbeFailure("input_path_not_absolute")
        source = args.adapter_source.resolve(strict=True)
        node = args.node.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        package = json.loads((source / "package.json").read_text())
        if package.get("name") != "@agentclientprotocol/claude-agent-acp" or package.get("version") != ADAPTER_VERSION:
            raise ProbeFailure("adapter_identity_mismatch")
        if installed_version(source, "@anthropic-ai/claude-agent-sdk") != SDK_VERSION:
            raise ProbeFailure("sdk_version_mismatch")
        adapter = (source / "dist/index.js").resolve(strict=True)
        if not adapter.is_file() or not adapter.is_relative_to(source):
            raise ProbeFailure("adapter_entry_invalid")
        result.update(run_probe(node, adapter))
        result.update(successful=True, adapterVersion=ADAPTER_VERSION, sdkVersion=SDK_VERSION)
    except ProbeFailure as error:
        result["failures"].append(error.category)
    except (OSError, ValueError, TypeError, AttributeError):
        result["failures"].append("probe_setup_failed")
    encoded = json.dumps(result, sort_keys=True) + "\n"
    args.output.write_text(encoded)
    print(encoded, end="")
    return 0 if result["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
