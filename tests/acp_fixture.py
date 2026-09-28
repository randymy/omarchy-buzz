#!/usr/bin/env python3
"""No relay/provider/harness launch: test supervisor's actual process boundary."""
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
loader = importlib.machinery.SourceFileLoader("fixture", str(ROOT / "scripts/acp-fixture"))
spec = importlib.util.spec_from_loader(loader.name, loader)
fixture = importlib.util.module_from_spec(spec)
loader.exec_module(fixture)
import json
valid_tag = json.dumps(["auth", fixture.OWNER, "kind=9", "a" * 128])
assert json.loads(fixture.auth_tag(valid_tag))[1] == fixture.OWNER
for tag in ("{}", json.dumps(["auth", fixture.OWNER, "kind=1", "a" * 128]), json.dumps(["auth", "b" * 64, "kind=9", "a" * 128])):
    try:
        fixture.auth_tag(tag)
        raise AssertionError("wrong owner or scope accepted")
    except ValueError:
        pass
for bad in ("wss://127.0.0.1:3000", "ws://example.com:3000", "ws://127.0.0.1", "ws://u:p@127.0.0.1:3000", "ws://127.0.0.1:3000/a", "ws://127.0.0.1:3000?q=1"):
    try:
        fixture.origin(bad)
        raise AssertionError("nonisolated origin accepted")
    except ValueError:
        pass
assert fixture.origin("ws://127.0.0.1:3000/") == "ws://127.0.0.1:3000"
assert fixture.origin("ws://[::1]:3000") == "ws://[::1]:3000"
with tempfile.TemporaryDirectory(prefix="buzz-acp-supervisor-test-") as temporary:
    root = Path(temporary)
    binaries = root / "bin"
    binaries.mkdir()
    os.environ["OPENAI_API_KEY"] = "synthetic-do-not-inherit"
    os.environ["DBUS_SESSION_BUS_ADDRESS"] = "synthetic-do-not-inherit"
    env = fixture.environment(root, binaries, Path(sys.executable), "ws://127.0.0.1:12345")
    assert "OPENAI_API_KEY" not in env and "DBUS_SESSION_BUS_ADDRESS" not in env
    assert env["BUZZ_PRIVATE_KEY"] == "0" * 63 + "2"
    assert all((root / name).stat().st_mode & 0o777 == 0o700 for name in ("home", "config", "runtime", "state", "data", "cache", "workspace"))
    command = fixture.command(binaries, Path(sys.executable), root / "peer.mjs", "11111111-1111-4111-8111-111111111111")
    assert "--respond-to" in command and command[command.index("--respond-to") + 1] == "owner-only"
    assert "--private-key" not in command and fixture.PUBLIC_FIXTURE_SCALAR not in command
    # Fixed mock child, not an ACP or model agent. Assert actual inherited environment.
    child = subprocess.Popen([sys.executable, "-c", "import os,time; assert 'OPENAI_API_KEY' not in os.environ; assert 'DBUS_SESSION_BUS_ADDRESS' not in os.environ; time.sleep(30)"], env=env, cwd=root / "workspace", start_new_session=True)
    time.sleep(0.1)
    fixture.cleanup(child)
    assert child.poll() is not None
    # A fixed mock descendant must be stopped with the harness-owned group.
    descendant_file = root / "descendant.pid"
    mock = "import pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'],process_group=0); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)"
    parent = subprocess.Popen([sys.executable, "-c", mock, str(descendant_file)], env=env, start_new_session=True)
    expires = time.monotonic() + 3
    while not descendant_file.exists() and time.monotonic() < expires:
        time.sleep(0.02)
    assert descendant_file.exists(), "mock descendant did not start"
    descendant = int(descendant_file.read_text())
    assert os.getpgid(descendant) == descendant
    assert os.getsid(descendant) == parent.pid
    fixture.cleanup(parent)
    expires = time.monotonic() + 3
    while time.monotonic() < expires:
        try:
            state = Path(f"/proc/{descendant}/stat").read_text().split(") ", 1)[1].split()[0]
        except FileNotFoundError:
            break
        if state == "Z":
            break  # Not running; the system subreaper owns its final wait.
        time.sleep(0.02)
    else:
        raise AssertionError("owned mock descendant still running")
    # A child that exits on its own is also safe to clean up.
    exited = subprocess.Popen([sys.executable, "-c", "pass"], env=env, start_new_session=True)
    assert exited.wait(timeout=3) == 0
    fixture.cleanup(exited)
    reply_root = root / "reply"
    reply_root.mkdir()
    reply_env = fixture.environment(reply_root, binaries, Path(sys.executable), "ws://127.0.0.1:12345", True)
    assert "BUZZ_PRIVATE_KEY" not in reply_env and "BUZZ_E2E_CLI_BIN" not in reply_env
    read_fd = fixture.identity_pipe()
    reply_command = fixture.command(binaries, Path(sys.executable), fixture.REPLY_PEER,
                                    "11111111-1111-4111-8111-111111111111", True, read_fd)
    assert "--private-key-fd" in reply_command and "--harness-replies" in reply_command
    assert "--deny-tool-requests" in reply_command and "read-only" in reply_command
    assert "--session-policy" in reply_command and "thread" in reply_command
    assert "--multiple-event-handling" in reply_command and "queue" in reply_command
    assert fixture.PUBLIC_FIXTURE_SCALAR not in " ".join(reply_command)
    # Exercise the same pass_fds boundary used to launch the real patched harness.
    fd_probe = "import os,sys; fd=int(sys.argv[1]); assert os.read(fd,128)==b'" + fixture.PUBLIC_FIXTURE_SCALAR + "\\n'; assert 'BUZZ_PRIVATE_KEY' not in os.environ; assert 'BUZZ_E2E_CLI_BIN' not in os.environ"
    child = subprocess.run([sys.executable, "-c", fd_probe, str(read_fd)], env=reply_env,
                           pass_fds=(read_fd,), capture_output=True, timeout=5)
    os.close(read_fd)
    assert child.returncode == 0, child.stderr
    try:
        fixture.command(binaries, Path(sys.executable), fixture.REPLY_PEER,
                        "11111111-1111-4111-8111-111111111111", True)
        raise AssertionError("reply mode accepted no key FD")
    except ValueError:
        pass

    peer_requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "session/new", "params": {}},
        {"jsonrpc": "2.0", "id": 3, "method": "session/set_config_option",
         "params": {"sessionId": "synthetic-reply-1", "configId": "mode", "value": "read-only"}},
        {"jsonrpc": "2.0", "id": 4, "method": "session/prompt",
         "params": {"sessionId": "synthetic-reply-1",
                    "prompt": [{"type": "text", "text": "AE-ID:TEST-123"}]}},
        {"jsonrpc": "2.0", "id": "fixture-permission",
         "result": {"outcome": {"outcome": "selected", "optionId": "deny"}}},
    ]
    peer = subprocess.run([sys.executable, str(fixture.REPLY_PEER)],
                          input="".join(json.dumps(item) + "\n" for item in peer_requests),
                          text=True, capture_output=True, env=reply_env, timeout=5)
    assert peer.returncode == 0 and peer.stderr == ""
    replies = [json.loads(line) for line in peer.stdout.splitlines()]
    assert len(replies) == 7
    assert replies[1]["result"]["modes"]["availableModes"] == [{"id": "read-only"}]
    assert replies[2]["result"]["configOptions"] == [{"id": "mode", "currentValue": "read-only"}]
    assert replies[3]["params"]["update"]["sessionUpdate"] == "agent_thought_chunk"
    assert replies[3]["params"]["update"]["content"]["text"] == "THOUGHT-MUST-NOT-PUBLISH"
    assert replies[4]["method"] == "session/request_permission"
    assert replies[4]["params"]["options"][1]["kind"] == "reject_once"
    assert replies[5]["params"]["sessionId"] == "synthetic-reply-1"
    assert replies[5]["params"]["update"]["content"]["text"] == "AE-ACK:TEST-123"
    assert replies[6]["result"]["stopReason"] == "end_turn"
    denied = [*peer_requests[:-1],
              {"jsonrpc": "2.0", "id": "fixture-permission",
               "result": {"outcome": {"outcome": "selected", "optionId": "allow"}}}]
    rejected_peer = subprocess.run([sys.executable, str(fixture.REPLY_PEER)],
                                   input="".join(json.dumps(item) + "\n" for item in denied),
                                   text=True, capture_output=True, env=reply_env, timeout=5)
    rejected_replies = [json.loads(line) for line in rejected_peer.stdout.splitlines()]
    assert rejected_peer.returncode == 0
    assert not any(item.get("method") == "session/update" and
                   item.get("params", {}).get("update", {}).get("sessionUpdate") == "agent_message_chunk"
                   for item in rejected_replies)
    assert rejected_replies[-1]["error"]["message"] == "fixture permission was not denied"
print("PASS: loopback-only fixture, public disposable identity, clean private environment, fixed owner routing and process cleanup")
