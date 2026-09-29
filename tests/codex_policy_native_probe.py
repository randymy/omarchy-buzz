#!/usr/bin/env python3
"""Account-free native config/read compatibility check for the proposed Codex policy.

Run only as non-root in a loopback-only network namespace. No account/read,
login, thread, model, prompt, browser, or provider request is sent.
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

from codex_subscription_probe import native_entry
from vendor_adapter_discovery import (
    MAX_CAPTURE, ProbeFailure, VERSION, capture_bounded, clean_child_env,
    installed_version, network_isolated,
)

NATIVE_VERSION = "0.158.0"
CANONICAL_CHATGPT_BASE_URL = "https://chatgpt.com/backend-api/"
TOTAL_TIMEOUT = 45
LINE_LIMIT = MAX_CAPTURE
REQUIRED_FLAGS = ("forcedChatgpt", "defaultOrOpenaiProvider",
                  "noCustomProviderDefinitions", "openaiBaseUrlAbsent",
                  "chatgptBaseUrlCanonical", "modelCatalogUrlAbsent",
                  "experimentalBearerTokenAbsent")
POLICY_SCRIPT = """import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const policy = await import(pathToFileURL(process.argv[1]).href);
const config = JSON.parse(readFileSync(process.argv[2], 'utf8'));
if (!policy.validSubscriptionConfig(config)) process.exit(4);
process.stdout.write('compatible\\n');
"""


def _send(process, payload):
    data = (json.dumps(payload, separators=(",", ":")) + "\n").encode()
    if len(data) > 4096:
        raise ProbeFailure("native_request_limit")
    try:
        process.stdin.write(data)
        process.stdin.flush()
    except (BrokenPipeError, OSError):
        raise ProbeFailure("native_stdin_closed") from None


def _response(message, expected_id):
    if not isinstance(message, dict) or type(message.get("id")) is not int or message["id"] != expected_id:
        raise ProbeFailure("native_response_invalid")
    result = message.get("result")
    if not isinstance(result, dict) or "error" in message:
        raise ProbeFailure("native_request_rejected")
    if expected_id == 1:
        return None
    config = result.get("config")
    if not isinstance(config, dict):
        raise ProbeFailure("config_response_invalid")
    return config


def _flags(config):
    providers = config.get("model_providers")
    chatgpt_base_url = config.get("chatgpt_base_url")
    return {
        "forcedChatgpt": config.get("forced_login_method") == "chatgpt",
        "defaultOrOpenaiProvider": config.get("model_provider") in (None, "openai"),
        "noCustomProviderDefinitions": providers is None or providers == {},
        "openaiBaseUrlAbsent": config.get("openai_base_url") is None,
        "chatgptBaseUrlPresent": chatgpt_base_url is not None,
        "chatgptBaseUrlExactDefault": chatgpt_base_url == CANONICAL_CHATGPT_BASE_URL,
        "chatgptBaseUrlCanonical": chatgpt_base_url is None
        or chatgpt_base_url == CANONICAL_CHATGPT_BASE_URL,
        "modelCatalogUrlAbsent": config.get("model_catalog_url") is None,
        "experimentalBearerTokenAbsent": config.get("experimental_bearer_token") is None,
    }


def read_native_config(node, entry, home, deadline):
    home.mkdir(mode=0o700, parents=True, exist_ok=True)
    codex_home = home / "codex"
    workspace = home / "workspace"
    codex_home.mkdir(mode=0o700)
    workspace.mkdir(mode=0o700)
    (codex_home / "config.toml").write_text('forced_login_method = "chatgpt"\n')
    env = clean_child_env(home, node)
    env["CODEX_HOME"] = str(codex_home)
    argv = [str(node), str(entry), "-c", "forced_login_method=chatgpt", "app-server"]
    try:
        process = subprocess.Popen(argv, cwd=workspace, env=env, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
    except OSError:
        raise ProbeFailure("native_spawn_failed") from None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    pending = bytearray()
    captured = 0
    expected_id = 1
    try:
        _send(process, {"id": 1, "method": "initialize", "params": {
            "clientInfo": {"name": "policy-config-probe", "version": "1.0.0"},
            "capabilities": {"experimentalApi": False},
        }})
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("native_timeout")
            if process.poll() is not None and not selector.get_map():
                raise ProbeFailure("native_exited_early")
            for key, _ in selector.select(min(remaining, 0.2)):
                try:
                    chunk = os.read(key.fileobj.fileno(), 4096)
                except OSError:
                    raise ProbeFailure("native_read_failed") from None
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                captured += len(chunk)
                if captured > MAX_CAPTURE:
                    raise ProbeFailure("native_output_limit")
                if key.data != "stdout":
                    continue
                pending.extend(chunk)
                if len(pending) > LINE_LIMIT:
                    raise ProbeFailure("native_line_limit")
                while b"\n" in pending:
                    line, _, rest = pending.partition(b"\n")
                    pending = bytearray(rest)
                    try:
                        message = json.loads(line)
                    except (UnicodeError, ValueError):
                        raise ProbeFailure("native_json_invalid") from None
                    if not isinstance(message, dict):
                        raise ProbeFailure("native_json_invalid")
                    if "id" not in message:
                        if "method" not in message:
                            raise ProbeFailure("native_notification_invalid")
                        continue
                    config = _response(message, expected_id)
                    if expected_id == 2:
                        return config
                    expected_id = 2
                    _send(process, {"method": "initialized"})
                    _send(process, {"id": 2, "method": "config/read",
                                    "params": {"cwd": str(workspace), "includeLayers": False}})
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


def check_policy_module(node, module, config, home, deadline):
    if module.name != "SubscriptionPolicy.ts" or not module.is_file():
        raise ProbeFailure("policy_module_unavailable")
    with tempfile.TemporaryDirectory(prefix="policy-check-", dir=home) as temporary:
        child_home = Path(temporary)
        config_path = child_home / "config-response.json"
        config_path.write_text(json.dumps(config, separators=(",", ":")))
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ProbeFailure("probe_timeout")
        raw = capture_bounded([str(node), "--experimental-strip-types", "--input-type=module",
                               "-e", POLICY_SCRIPT, str(module), str(config_path)],
                              clean_child_env(child_home, node), child_home, min(5, remaining))
        if raw != b"compatible\n":
            raise ProbeFailure("policy_module_mismatch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packages", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--policy-module", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    summary = {"successful": False, "networkIsolated": False,
               "nativePackageVersion": None, "nativeBinaryVersion": None,
               "nodeVersion": None, "configReadObserved": False,
               "policyModuleAccepted": False, "noAccountRead": True,
               "noThreadOrModelRequest": True, "flags": None, "failures": []}
    deadline = time.monotonic() + TOTAL_TIMEOUT
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not all(p.is_absolute() for p in (args.packages, args.node, args.policy_module)):
            raise ProbeFailure("input_path_not_absolute")
        packages = args.packages.resolve(strict=True)
        node = args.node.resolve(strict=True)
        module = args.policy_module.resolve(strict=True)
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        version = installed_version(packages, "@openai/codex")
        summary["nativePackageVersion"] = version
        if version != NATIVE_VERSION:
            raise ProbeFailure("native_package_version_mismatch")
        entry = native_entry(packages)
        with tempfile.TemporaryDirectory(prefix="codex-policy-version-") as temporary:
            home = Path(temporary)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), "--version"], clean_child_env(home, node), home,
                                  min(5, remaining))
        node_version = raw.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(node_version) or int(node_version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = node_version
        with tempfile.TemporaryDirectory(prefix="codex-policy-native-version-") as temporary:
            home = Path(temporary)
            env = clean_child_env(home, node)
            env["CODEX_HOME"] = str(home / "codex")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProbeFailure("probe_timeout")
            raw = capture_bounded([str(node), str(entry), "--version"], env, home, min(5, remaining))
        if raw.decode("ascii", errors="ignore").strip() != "codex-cli " + NATIVE_VERSION:
            raise ProbeFailure("native_binary_version_mismatch")
        summary["nativeBinaryVersion"] = NATIVE_VERSION
        with tempfile.TemporaryDirectory(prefix="codex-policy-native-") as temporary:
            home = Path(temporary)
            config = read_native_config(node, entry, home, deadline)
            summary["configReadObserved"] = True
            summary["flags"] = _flags(config)
            check_policy_module(node, module, config, home, deadline)
        summary["policyModuleAccepted"] = True
        summary["successful"] = all(summary["flags"][key] for key in REQUIRED_FLAGS)
        if not summary["successful"]:
            raise ProbeFailure("config_shape_incompatible")
    except ProbeFailure as error:
        summary["failures"].append(error.category)
    except (OSError, ValueError):
        summary["failures"].append("probe_setup_failed")
    args.output.write_text(json.dumps(summary, sort_keys=True) + "\n")
    print(json.dumps(summary, sort_keys=True))
    return 0 if summary["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
