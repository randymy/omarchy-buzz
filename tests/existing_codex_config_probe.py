"""Read subscription routing flags from a selected native profile, without a turn."""
import argparse
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import time
from codex_policy_native_probe import query_native_config, _flags, REQUIRED_FLAGS
from vendor_adapter_discovery import ProbeFailure

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bundle', type=Path, required=True)
parser.add_argument('--manifest-sha256', required=True)
args = parser.parse_args()
loader = importlib.machinery.SourceFileLoader('preview', str(Path(__file__).resolve().parents[1]/'scripts/agent-preview'))
spec = importlib.util.spec_from_loader(loader.name, loader)
preview = importlib.util.module_from_spec(spec)
loader.exec_module(preview)
report = {'successful': False, 'profileMode': 'existing', 'sessionCreated': False,
          'modelTaskSubmitted': False, 'configurationWritten': False}
try:
    manifest = preview.verify_bundle(args.bundle, args.manifest_sha256)
    preview.verify_runtime(args.bundle, manifest, 'codex')
    profile = preview.profile_for('codex', args.bundle, manifest)
    provider = preview.selected_provider('codex', 'existing', None, profile)
    env = preview.environment(profile, args.bundle, {})
    env['CODEX_HOME'] = str(provider)
    argv = preview.login_command(args.bundle, 'codex')[:-1] + ['app-server']
    config = query_native_config(argv, env, profile/'work', time.monotonic()+30)
    report['routingFlags'] = _flags(config)
    report['successful'] = all(report['routingFlags'][key] for key in REQUIRED_FLAGS)
except (ProbeFailure, preview.Refused) as error:
    report['error'] = error.category if isinstance(error, ProbeFailure) else str(error)
except (OSError, ValueError, KeyError, TypeError):
    report['error'] = 'config_check_failed'
print(json.dumps(report))
raise SystemExit(0 if report['successful'] else 1)
