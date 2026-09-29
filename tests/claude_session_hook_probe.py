#!/usr/bin/env python3
"""Account-free Claude session/new hook comparison in an isolated net namespace.

Use: unshare --user --map-root-user --net python3 tests/claude_session_hook_probe.py
     --bundle BUNDLE --outside-netns NETNS --host-uid UID
No prompt, login, real account, or relay is used. Output contains booleans only.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import tempfile
import time

from claude_existing_discovery_probe import MANIFEST_SHA256, checked_file, network_isolated
from vendor_adapter_discovery import clean_child_env


def session(node, adapter, env, work, guarded):
    argv = [str(node), str(adapter)] + (["--require-claude-subscription"] if guarded else [])
    child = subprocess.Popen(argv, cwd=work, env=env, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=True)
    poll = selectors.DefaultSelector()
    poll.register(child.stdout, selectors.EVENT_READ, "stdout")
    poll.register(child.stderr, selectors.EVENT_READ, "stderr")
    pending = bytearray()
    result = {"initialized": False, "sessionRequestSent": False,
              "sessionResponded": False, "sessionCreated": False,
              "clientRequestsDenied": 0, "nativeProcessObserved": False}
    deadline = time.monotonic() + 35

    def send(value):
        child.stdin.write((json.dumps(value, separators=(",", ":")) + "\n").encode())
        child.stdin.flush()

    try:
        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": 1, "clientCapabilities": {
                "fs": {"readTextFile": False, "writeTextFile": False},
                "terminal": False, "auth": {"terminal": True}}}})
        while time.monotonic() < deadline and poll.get_map():
            children = Path(f"/proc/{child.pid}/task/{child.pid}/children")
            if children.exists():
                for pid in children.read_text().split():
                    comm = Path(f"/proc/{pid}/comm")
                    if comm.exists() and "claude" in comm.read_text().lower():
                        result["nativeProcessObserved"] = True
            for key, _ in poll.select(0.02):
                chunk = os.read(key.fileobj.fileno(), 4096)
                if not chunk:
                    poll.unregister(key.fileobj)
                    continue
                if key.data == "stderr":
                    continue
                pending.extend(chunk)
                if len(pending) > 1024 * 1024:
                    raise AssertionError("ACP output limit")
                while b"\n" in pending:
                    line, _, rest = pending.partition(b"\n")
                    pending = bytearray(rest)
                    message = json.loads(line)
                    if "method" in message:
                        if "id" in message:
                            result["clientRequestsDenied"] += 1
                            send({"jsonrpc": "2.0", "id": message["id"],
                                  "error": {"code": -32601, "message": "Disabled"}})
                        continue
                    if message.get("id") == 1:
                        assert isinstance(message.get("result"), dict)
                        result["initialized"] = True
                        send({"jsonrpc": "2.0", "id": 2, "method": "session/new",
                              "params": {"cwd": str(work), "mcpServers": []}})
                        result["sessionRequestSent"] = True
                    elif message.get("id") == 2:
                        result["sessionResponded"] = True
                        result["sessionCreated"] = isinstance(
                            message.get("result", {}).get("sessionId"), str)
                        error = message.get("error")
                        result["subscriptionRejected"] = (
                            isinstance(error, dict) and
                            error.get("data") == {
                                "reason": "claude_subscription_route_unavailable"})
                        return result
        result["timedOut"] = True
        return result
    finally:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=5)
        poll.close()
        for stream in (child.stdin, child.stdout, child.stderr):
            stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--outside-netns", required=True)
    parser.add_argument("--host-uid", type=int, required=True)
    args = parser.parse_args()
    assert network_isolated(args.outside_netns, args.host_uid)
    bundle = args.bundle.resolve(strict=True)
    manifest_path = bundle / "manifest.json"
    assert hashlib.sha256(manifest_path.read_bytes()).hexdigest() == MANIFEST_SHA256
    manifest = json.loads(manifest_path.read_text())
    assert manifest["sources"]["claude"]["revision"] == "18de37624071b48e95aed9ec5382823e2d72cd39"
    node = checked_file(bundle, manifest, "bin/node")
    adapter = checked_file(bundle, manifest, "adapters/claude/dist/index.js")
    assert json.loads((bundle / "adapters/claude/package.json").read_text())["version"] == "0.82.0"
    with tempfile.TemporaryDirectory(prefix="claude-session-hook-") as temporary:
        root = Path(temporary)
        results = {}
        for name, guarded in (("ordinary", False), ("guarded", True)):
            home = root / name
            home.mkdir(mode=0o700)
            config = home / "claude"
            config.mkdir(mode=0o700)
            work = home / "work"
            work.mkdir(mode=0o700)
            marker = home / "hook-executed"
            (config / "settings.json").write_text(json.dumps({
                "hooks": {"SessionStart": [{"hooks": [{
                    "type": "command", "command": "touch " + str(marker)}]}]}}))
            env = clean_child_env(home, node)
            env["CLAUDE_CONFIG_DIR"] = str(config)
            results[name] = session(node, adapter, env, work, guarded)
            results[name]["hookCommandExecuted"] = marker.exists()
        assert results["ordinary"]["initialized"]
        assert results["ordinary"]["sessionCreated"]
        assert results["ordinary"]["nativeProcessObserved"]
        assert results["ordinary"]["hookCommandExecuted"]
        assert results["guarded"]["initialized"]
        assert results["guarded"]["sessionResponded"]
        assert results["guarded"]["nativeProcessObserved"]
        assert results["guarded"]["subscriptionRejected"]
        assert not results["guarded"]["hookCommandExecuted"]
    print(json.dumps({"networkIsolated": True, "syntheticProfile": True,
                      "modelPromptSent": False, "results": results}))


if __name__ == "__main__":
    main()
