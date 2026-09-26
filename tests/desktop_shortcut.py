#!/usr/bin/env python3
"""Fixture-only shortcut transactions; never runs the real hyprctl."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MOCK = '''#!/usr/bin/python3
import json, os, pathlib, sys
root = pathlib.Path(os.environ["XDG_CONFIG_HOME"])
with (root / "calls").open("a") as f: f.write(" ".join(sys.argv[1:]) + "\\n")
if sys.argv[1:] == ["-j", "binds"]:
    if (root / "unreachable").exists(): sys.exit(1)
    if (root / "bad-json").exists(): print("invalid")
    elif (root / "conflict").exists(): print(json.dumps([{"modmask":64,"key":"B","dispatcher":"exec","arg":"unowned","description":"Other"}]))
    elif "-- BEGIN omarchy-buzz shortcut v1" in (root / "hypr/bindings.lua").read_text():
        print(json.dumps([{"modmask":64,"key":"B","dispatcher":"__lua","arg":"208","description":"Buzz"}]))
    else: print("[]")
elif sys.argv[1:] == ["configerrors"]: print("" if not (root / "errors").exists() else "synthetic error")
elif sys.argv[1:] == ["reload"]: print("ok")
else: sys.exit(2)
'''

with tempfile.TemporaryDirectory(prefix="buzz-shortcut-test-") as temporary:
    root = Path(temporary)
    config = root / "config"
    (config / "hypr").mkdir(parents=True)
    binary = root / "bin"
    binary.mkdir()
    mock = binary / "hyprctl"
    mock.write_text(MOCK)
    mock.chmod(0o700)
    target = config / "hypr/bindings.lua"
    original = b'-- unrelated bytes\r\no.bind("SUPER + Q", "Fixture", "fixed")\n-- no final newline'
    target.write_bytes(original)
    target.chmod(0o640)
    env = {"PATH": str(binary) + ":/usr/bin", "HOME": str(root), "XDG_CONFIG_HOME": str(config)}
    def run(action, success=True):
        result = subprocess.run([str(ROOT / "scripts/desktop-shortcut"), action], env=env, capture_output=True, text=True)
        assert (result.returncode == 0) == success, (action, result.stdout, result.stderr)
        return result.stdout
    assert run("status").strip() == "available"
    assert not (target.parent / ".omarchy-buzz-shortcut.lock").exists()
    for marker in ("conflict", "unreachable", "bad-json"):
        (config / marker).touch()
        run("install", False)
        assert target.read_bytes() == original
        assert not list(target.parent.glob("*.omarchy-buzz-backup-*"))
        (config / marker).unlink()
    run("install")
    installed = target.read_bytes()
    assert installed.startswith(original)
    assert target.stat().st_mode & 0o777 == 0o640
    backups = list(target.parent.glob("*.omarchy-buzz-backup-*"))
    assert len(backups) == 1 and backups[0].read_bytes() == original
    assert backups[0].stat().st_mode & 0o777 == 0o600
    assert run("status").strip() == "installed"
    run("install")
    assert target.read_bytes() == installed and len(list(target.parent.glob("*.omarchy-buzz-backup-*"))) == 1
    target.write_bytes(installed.replace(b'"Buzz"', b'"Changed"'))
    run("remove", False)
    assert b'"Changed"' in target.read_bytes()
    target.write_bytes(installed + b'\n-- later unrelated user edit\n')
    run("remove")
    assert target.read_bytes() == original + b'\n-- later unrelated user edit\n'
    assert run("status").strip() == "available"
    run("remove")
    target.write_bytes(original)
    (config / "errors").touch()
    run("install", False)
    assert target.read_bytes().startswith(original) and b'BEGIN omarchy-buzz' in target.read_bytes()
    assert "reload\nconfigerrors\n" in (config / "calls").read_text()
    (config / "errors").unlink()
    run("remove")
    target.unlink()
    target.symlink_to(config / "calls")
    run("install", False)
print("PASS: fixed shortcut conflicts, unreachable/malformed IPC, preservation, backups, idempotency, modified-block refusal, validation and symlink refusal")
