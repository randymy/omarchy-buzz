"""Account-free native CLI check; takes a verified Claude binary path."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
args = parser.parse_args()
assert args.binary.is_absolute() and args.binary.is_file()
with tempfile.TemporaryDirectory(prefix="buzz-claude-status-") as directory:
    root = Path(directory)
    config = root / "provider"
    config.mkdir(mode=0o700)
    marker = root / "unexpected-command"
    command = "touch " + str(marker)
    (config / "settings.json").write_text(json.dumps({
        "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": command}]}]},
        "apiKeyHelper": command,
    }))
    result = subprocess.run([
        str(args.binary), "--setting-sources", "", "--settings", '{"disableAllHooks":true}',
        "auth", "status", "--json",
    ], cwd=root, env={"HOME": str(root), "CLAUDE_CONFIG_DIR": str(config),
                      "PATH": "/usr/bin:/bin"}, capture_output=True, text=True, timeout=20)
    value = json.loads(result.stdout)
    assert result.returncode == 1 and value.get("loggedIn") is False
    assert not marker.exists(), "fixture command executed"
    print(json.dumps({"successful": True, "fixtureCommandsExecuted": False,
                      "realAccountUsed": False, "modelTaskSubmitted": False}))
