#!/usr/bin/env python3
"""Account-free guarded Claude auth-method discovery with a synthetic user hook.

Run inside a loopback-only network namespace. This sends only ACP initialize via
buzz-acp auth-methods; it never authenticates, creates a session, or prompts.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile

from vendor_adapter_discovery import capture_bounded, clean_child_env


MANIFEST_SHA256 = "dccd9e9a0659f53a7c910a0cfc8fe1bb000277213447284d7f1090f7e450b2f9"
EXPECTED_METHODS = [{"id": "claude-ai-login", "type": "terminal"}]


def network_isolated(outside_netns, host_uid):
    # unshare --user --map-root-user --net maps only the invoking host user.
    # uid 0 inside this namespace is not host root.
    assert os.geteuid() == 0
    assert Path("/proc/self/uid_map").read_text().split()[:3] == ["0", str(host_uid), "1"]
    assert os.readlink("/proc/self/ns/net") != outside_netns
    interfaces = {line.split(":", 1)[0].strip() for line in
                  Path("/proc/net/dev").read_text().splitlines()[2:] if ":" in line}
    return interfaces == {"lo"}


def checked_file(bundle, manifest, name):
    target = bundle / name
    entry = manifest["files"][name]
    assert target.is_file() and not target.is_symlink()
    assert target.stat().st_size == entry["bytes"]
    assert hashlib.sha256(target.read_bytes()).hexdigest() == entry["sha256"]
    return target


def discover(bundle, provider, work):
    env = clean_child_env(work, bundle / "bin/node")
    env["CLAUDE_CONFIG_DIR"] = str(provider)
    command = [str(bundle / "bin/buzz-acp"), "auth-methods",
               "--agent-command", str(bundle / "bin/node"), "--agent-args",
               str(bundle / "adapters/claude/dist/index.js") + ",--require-claude-subscription",
               "--json", "--terminal-auth"]
    raw = capture_bounded(command, env, work, 45)
    methods = json.loads(raw)["methods"]
    assert [{"id": m["id"], "type": m.get("type", "agent")} for m in methods] == EXPECTED_METHODS
    return methods


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--host-uid", type=int, required=True)
    args = parser.parse_args()
    bundle = args.bundle.resolve(strict=True)
    assert network_isolated(args.outside_netns, args.host_uid)
    manifest_path = bundle / "manifest.json"
    assert hashlib.sha256(manifest_path.read_bytes()).hexdigest() == MANIFEST_SHA256
    manifest = json.loads(manifest_path.read_text())
    assert manifest["sources"]["claude"]["revision"] == "18de37624071b48e95aed9ec5382823e2d72cd39"
    for name in ("bin/buzz-acp", "bin/node", "adapters/claude/dist/index.js"):
        checked_file(bundle, manifest, name)
    adapter = bundle / "adapters/claude"
    assert json.loads((adapter / "package.json").read_text())["version"] == "0.82.0"
    assert json.loads((adapter / "node_modules/@anthropic-ai/claude-agent-sdk/package.json").read_text())["version"] == "0.3.280"
    with tempfile.TemporaryDirectory(prefix="claude-existing-discovery-") as temporary:
        root = Path(temporary)
        provider = root / "existing-claude"
        provider.mkdir(mode=0o700)
        baseline = root / "baseline"
        baseline.mkdir(mode=0o700)
        baseline_methods = discover(bundle, provider, baseline)
        marker = root / "hook-executed"
        settings = {"hooks": {"SessionStart": [{"hooks": [{
            "type": "command", "command": "touch " + str(marker),
        }]}]}}
        (provider / "settings.json").write_text(json.dumps(settings))
        with_hook = root / "with-hook"
        with_hook.mkdir(mode=0o700)
        hooked_methods = discover(bundle, provider, with_hook)
        assert hooked_methods == baseline_methods
        assert not marker.exists(), "SessionStart hook command executed during discovery"
    print(json.dumps({"successful": True, "networkIsolated": True,
                      "guardedMethods": EXPECTED_METHODS, "hookCommandExecuted": False,
                      "realAccountUsed": False, "loginStarted": False,
                      "sessionCreated": False, "modelTaskSubmitted": False}))


if __name__ == "__main__":
    main()
