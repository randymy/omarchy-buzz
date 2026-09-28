#!/usr/bin/env python3
"""Offline, read-only ACP method discovery against pinned installed adapters.

Run only inside a separate, loopback-only network namespace as a non-root user.
This invokes buzz-acp *auth-methods* only. It never authenticates or starts a
session, prompt, browser, or relay connection. Provider requests cannot leave
the network namespace.
"""

import argparse
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import tempfile
import time


MAX_CAPTURE = 64 * 1024
PROBE_TIMEOUT = 45
METHOD_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$")
VERSION = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")
EXPECTED = {
    "codex": ("@agentclientprotocol/codex-acp", "2.0.0", "codex-acp"),
    "claude": ("@agentclientprotocol/claude-agent-acp", "0.82.0", "claude-agent-acp"),
}
NATIVE = {
    "@openai/codex": "0.158.0",
    "@anthropic-ai/claude-agent-sdk": "0.3.280",
}


class ProbeFailure(Exception):
    def __init__(self, category):
        super().__init__(category)
        self.category = category


def network_isolated(outside_netns):
    if os.geteuid() == 0:
        raise ProbeFailure("root_forbidden")
    try:
        own_netns = os.readlink("/proc/self/ns/net")
    except OSError:
        raise ProbeFailure("netns_unavailable") from None
    if not re.fullmatch(r"net:\[[0-9]+\]", outside_netns):
        raise ProbeFailure("outside_netns_invalid")
    if own_netns == outside_netns:
        raise ProbeFailure("network_namespace_not_isolated")
    try:
        # /proc is mounted inside the namespace by the runner. /sys can retain
        # a host view after unshare, so it is not evidence of this namespace.
        proc_interfaces = {
            line.split(":", 1)[0].strip()
            for line in Path("/proc/net/dev").read_text().splitlines()[2:]
            if ":" in line
        }
    except OSError:
        raise ProbeFailure("network_interfaces_unavailable") from None
    if not proc_interfaces:
        raise ProbeFailure("network_interfaces_unavailable")
    if not proc_interfaces.issubset({"lo"}):
        raise ProbeFailure("network_not_loopback_only")
    return True


def installed_version(packages, package):
    manifest = packages / "node_modules" / package / "package.json"
    try:
        data = json.loads(manifest.read_text())
    except (OSError, ValueError):
        raise ProbeFailure("package_manifest_unavailable") from None
    if not isinstance(data, dict) or data.get("name") != package:
        raise ProbeFailure("package_identity_mismatch")
    version = data.get("version")
    if not isinstance(version, str) or len(version) > 40:
        raise ProbeFailure("package_version_invalid")
    return version


def adapter_executable(packages, name):
    link = packages / "node_modules" / ".bin" / name
    try:
        target = link.resolve(strict=True)
        target.relative_to(packages.resolve(strict=True))
    except (OSError, ValueError):
        raise ProbeFailure("adapter_executable_outside_packages") from None
    if not target.is_file() or not os.access(target, os.X_OK):
        raise ProbeFailure("adapter_executable_unavailable")
    return link


def clean_child_env(home, node):
    dirs = {}
    for key, name in (
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ):
        directory = home / name
        directory.mkdir(mode=0o700)
        dirs[key] = str(directory)
    return {
        "HOME": str(home),
        "PATH": os.pathsep.join(dict.fromkeys((str(node.parent), "/usr/bin", "/bin"))),
        "LANG": "C.UTF-8",
        "TERM": "dumb",
        "TMPDIR": str(home),
        **dirs,
    }


def capture_bounded(argv, env, cwd, timeout_seconds):
    try:
        process = subprocess.Popen(
            argv,
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError:
        raise ProbeFailure("spawn_failed") from None
    stdout = bytearray()
    stderr = bytearray()
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, stdout)
    selector.register(process.stderr, selectors.EVENT_READ, stderr)
    deadline = time.monotonic() + timeout_seconds
    category = None
    try:
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                category = "probe_timeout"
                break
            for key, _ in selector.select(min(remaining, 0.2)):
                chunk = os.read(key.fileobj.fileno(), 4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                key.data.extend(chunk)
                if len(stdout) + len(stderr) > MAX_CAPTURE:
                    category = "probe_output_limit"
                    break
            if category:
                break
        if not category:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                category = "probe_timeout"
            else:
                try:
                    process.wait(timeout=remaining)
                except subprocess.TimeoutExpired:
                    category = "probe_timeout"
        if category:
            raise ProbeFailure(category)
        if process.returncode != 0:
            raise ProbeFailure("probe_nonzero_exit")
        return bytes(stdout)
    finally:
        # Kill the probe process group on every path. Buzz manages its adapter
        # group separately; the outer PID namespace reaps any remaining children.
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


def sanitize_methods(raw):
    try:
        payload = json.loads(raw)
    except (UnicodeError, ValueError):
        raise ProbeFailure("methods_json_invalid") from None
    methods = payload.get("methods") if isinstance(payload, dict) else None
    if not isinstance(methods, list) or len(methods) > 64:
        raise ProbeFailure("methods_shape_invalid")
    sanitized = []
    seen = set()
    for method in methods:
        if not isinstance(method, dict):
            raise ProbeFailure("method_shape_invalid")
        method_id = method.get("id")
        if not isinstance(method_id, str) or not METHOD_ID.fullmatch(method_id):
            raise ProbeFailure("method_id_invalid")
        if method_id in seen:
            raise ProbeFailure("method_id_duplicate")
        seen.add(method_id)
        method_type = method.get("type", "agent")
        if not isinstance(method_type, str) or method_type not in ("agent", "terminal"):
            raise ProbeFailure("method_type_unsupported")
        sanitized.append({"id": method_id, "type": method_type})
    return sanitized


def run_probe(binary, node, executable, vendor, terminal_auth):
    with tempfile.TemporaryDirectory(prefix=f"acp-{vendor}-discovery-") as temporary:
        home = Path(temporary)
        env = clean_child_env(home, node)
        argv = [
            str(binary), "auth-methods",
            "--agent-command", str(executable),
            "--agent-args", "",
            "--json",
        ]
        if terminal_auth:
            argv.append("--terminal-auth")
        raw = capture_bounded(argv, env, home, PROBE_TIMEOUT)
    return sanitize_methods(raw)


def require_methods(results):
    codex_base = results["codex"]["default"]
    codex_opt = results["codex"]["terminalOptIn"]
    for methods in (codex_base, codex_opt):
        by_id = {entry["id"]: entry["type"] for entry in methods}
        if by_id.get("api-key") != "agent" or by_id.get("chat-gpt") != "agent":
            raise ProbeFailure("codex_expected_agent_methods_missing")
    claude_base = results["claude"]["default"]
    claude_opt = results["claude"]["terminalOptIn"]
    if any(entry["type"] == "terminal" for entry in claude_base):
        raise ProbeFailure("claude_terminal_without_opt_in")
    by_id = {entry["id"]: entry["type"] for entry in claude_opt}
    # The clean fixture excludes remote/SSH markers: require both local choices.
    if by_id.get("console-login") != "terminal" or by_id.get("claude-ai-login") != "terminal":
        raise ProbeFailure("claude_expected_terminal_methods_missing")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--packages", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    summary = {
        "successful": False,
        "networkIsolated": False,
        "packages": {},
        "nodeVersion": None,
        "methods": {},
        "failures": [],
    }
    active_case = None
    try:
        summary["networkIsolated"] = network_isolated(args.outside_netns)
        if not all(path.is_absolute() for path in (args.binary, args.packages, args.node)):
            raise ProbeFailure("input_path_not_absolute")
        binary = args.binary.resolve(strict=True)
        node = args.node.resolve(strict=True)
        packages = args.packages.resolve(strict=True)
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise ProbeFailure("buzz_binary_unavailable")
        if not node.is_file() or not os.access(node, os.X_OK):
            raise ProbeFailure("node_binary_unavailable")
        with tempfile.TemporaryDirectory(prefix="acp-node-version-") as temporary:
            home = Path(temporary)
            version = capture_bounded([str(node), "--version"], clean_child_env(home, node), home, 5)
        node_version = version.decode("ascii", errors="ignore").strip()
        if not VERSION.fullmatch(node_version):
            raise ProbeFailure("node_version_invalid")
        if int(node_version[1:].split(".", 1)[0]) < 22:
            raise ProbeFailure("node_version_unsupported")
        summary["nodeVersion"] = node_version
        expected_packages = {identity: version for identity, version, _ in EXPECTED.values()}
        expected_packages.update(NATIVE)
        for package, expected in expected_packages.items():
            observed = installed_version(packages, package)
            summary["packages"][package] = observed
            if observed != expected:
                raise ProbeFailure("package_version_mismatch")
        executables = {
            vendor: adapter_executable(packages, executable)
            for vendor, (_, _, executable) in EXPECTED.items()
        }
        for vendor, executable in executables.items():
            summary["methods"][vendor] = {}
            for label, terminal_auth in (("default", False), ("terminalOptIn", True)):
                active_case = f"{vendor}:{label}"
                summary["methods"][vendor][label] = run_probe(
                    binary, node, executable, vendor, terminal_auth
                )
                active_case = None
        require_methods(summary["methods"])
        summary["successful"] = True
    except ProbeFailure as error:
        summary["failures"].append(error.category)
        if active_case:
            summary["failedCase"] = active_case
    except (OSError, ValueError):
        summary["failures"].append("probe_setup_failed")
    args.output.write_text(json.dumps(summary, sort_keys=True) + "\n")
    print(json.dumps(summary, sort_keys=True))
    return 0 if summary["successful"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
