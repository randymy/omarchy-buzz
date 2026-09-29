#!/usr/bin/env python3
"""Bounded ACP session admission check. Never sends a model prompt or logs payloads."""
import argparse
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import tempfile
import time

from vendor_adapter_discovery import ProbeFailure
from codex_policy_native_probe import query_native_config, _flags, REQUIRED_FLAGS
from codex_policy_adapter_probe import require_response as codex_response
from claude_policy_adapter_probe import require_response as claude_response


def client_reply(message):
    if message.get('method') == 'session/request_permission':
        return {'jsonrpc': '2.0', 'id': message['id'], 'result': {'outcome': {'outcome': 'cancelled'}}}
    return {'jsonrpc': '2.0', 'id': message['id'], 'error': {'code': -32601, 'message': 'Client operation disabled'}}


def run(command, env, cwd, agent, report, timeout=60):
    child = subprocess.Popen(command, env=env, cwd=cwd, stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    selector = selectors.DefaultSelector()
    selector.register(child.stdout, selectors.EVENT_READ, 'stdout')
    selector.register(child.stderr, selectors.EVENT_READ, 'stderr')
    pending = bytearray()
    total = 0
    deadline = time.monotonic() + timeout
    expected = 1
    def send(value):
        child.stdin.write(json.dumps(value).encode() + b'\n')
        child.stdin.flush()
    try:
        send({'jsonrpc':'2.0','id':1,'method':'initialize','params':{
            'protocolVersion':1,'clientCapabilities':{'fs':{'readTextFile':False,'writeTextFile':False},
                                                    'terminal':False, 'auth':{'terminal':True}}}})
        while time.monotonic() < deadline:
            if not selector.get_map():
                raise ProbeFailure('adapter_exited')
            for key, _ in selector.select(0.2):
                chunk = os.read(key.fileobj.fileno(),4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                total += len(chunk)
                if total > 2*1024*1024:
                    raise ProbeFailure('output_limit')
                if key.data == 'stderr':
                    continue
                pending.extend(chunk)
                while b'\n' in pending:
                    line, _, rest = pending.partition(b'\n')
                    pending = bytearray(rest)
                    value = json.loads(line)
                    if not isinstance(value,dict):
                        raise ProbeFailure('invalid_response')
                    if 'method' in value:
                        if 'id' in value:
                            report['clientRequestsDenied'] += 1
                            send(client_reply(value))
                        continue
                    if type(value.get('id')) is not int or value['id'] != expected:
                        raise ProbeFailure('unexpected_response')
                    if expected in (1,2):
                        (codex_response if agent == 'codex' else claude_response)(value,expected)
                        if expected == 1:
                            report['initialized'] = True
                            expected = 2
                            send({'jsonrpc':'2.0','id':2,'method':'authenticate','params':{'methodId':'api-key'}})
                        else:
                            report['apiAuthenticationRejected'] = True
                            expected = 3
                            report['sessionRequestSent'] = True
                            send({'jsonrpc':'2.0','id':3,'method':'session/new','params':{'cwd':str(cwd),'mcpServers':[]}})
                    else:
                        result = value.get('result')
                        if 'error' in value:
                            raise ProbeFailure('session_admission_rejected')
                        if not isinstance(result,dict) or not isinstance(result.get('sessionId'),str) or not result['sessionId']:
                            raise ProbeFailure('session_response_invalid')
                        report['sessionCreated'] = True
                        return
        raise ProbeFailure('admission_timeout')
    finally:
        try:
            os.killpg(child.pid,signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=5)
        selector.close()
        for stream in (child.stdin,child.stdout,child.stderr):
            stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle',type=Path,required=True)
    parser.add_argument('--manifest-sha256',required=True)
    parser.add_argument('--agent',choices=['codex','claude'],required=True)
    parser.add_argument('--profile-mode',choices=['separate','existing'],default='separate')
    args = parser.parse_args()
    loader = importlib.machinery.SourceFileLoader('preview',str(Path(__file__).resolve().parents[1]/'scripts/agent-preview'))
    spec = importlib.util.spec_from_loader(loader.name,loader)
    preview = importlib.util.module_from_spec(spec)
    loader.exec_module(preview)
    report = {'agent':args.agent,'profileMode':args.profile_mode,'initialized':False,
              'apiAuthenticationRejected':False,'sessionRequestSent':False,'sessionCreated':False,
              'clientRequestsDenied':0,'modelPromptSent':False,'roomAgentEnabled':False,'successful':False}
    try:
        os.umask(0o077)
        preview.resource.setrlimit(preview.resource.RLIMIT_CORE,(0,0))
        if os.geteuid()==0:
            raise ProbeFailure('host_root_forbidden')
        manifest = preview.verify_bundle(args.bundle,args.manifest_sha256)
        preview.verify_runtime(args.bundle,manifest,args.agent)
        profile = preview.profile_for(args.agent,args.bundle,manifest)
        provider = preview.selected_provider(args.agent,args.profile_mode,None,profile)
        env = preview.environment(profile,args.bundle,{})
        env['CODEX_HOME' if args.agent=='codex' else 'CLAUDE_CONFIG_DIR'] = str(provider)
        with tempfile.TemporaryDirectory(prefix='admission-',dir=profile/'work') as work:
            if args.agent == 'claude':
                raise ProbeFailure('claude_session_hook_validation_required')
            config = query_native_config(preview.login_command(args.bundle, 'codex')[:-1]+['app-server'],
                                         env,Path(work),time.monotonic()+30)
            flags = _flags(config)
            if not all(flags[key] for key in REQUIRED_FLAGS):
                raise ProbeFailure('subscription_routing_rejected')
            if any(config.get(key) for key in ('mcp_servers','hooks','plugins','projects','skills')):
                raise ProbeFailure('native_integrations_review_required')
            report['effectiveConfigPreflight'] = 'passed'
            run([str(args.bundle/'bin/node'),str(args.bundle/preview.ENTRIES[args.agent]),preview.GUARDS[args.agent]],
                env,Path(work),args.agent,report)
        report['successful'] = True
    except (ProbeFailure,preview.Refused) as error:
        report['error'] = error.category if isinstance(error,ProbeFailure) else str(error)
    except (OSError,ValueError,TypeError,KeyError,subprocess.TimeoutExpired):
        report['error'] = 'admission_probe_failed'
    print(json.dumps(report))
    return 0 if report['successful'] else 1

if __name__=='__main__':
    raise SystemExit(main())
