#!/usr/bin/env python3
"""Exercise the real CLI authenticate path with a delayed synthetic ACP peer."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

PEER = '''import json,time
while True:
 try:r=json.loads(input())
 except EOFError:break
 if r.get('method')=='initialize':
  print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'protocolVersion':1,'agentCapabilities':{},'authMethods':[{'id':'synthetic','name':'Synthetic'}]}}),flush=True)
 elif r.get('method')=='authenticate':
  time.sleep(65)
  print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{}}),flush=True)
'''
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
with tempfile.TemporaryDirectory(prefix='acp-auth-delay-') as directory:
    home=Path(directory)
    peer=home/'peer.py';peer.write_text(PEER)
    started=time.monotonic()
    result=subprocess.run([str(args.binary.resolve()),'authenticate','--agent-command',sys.executable,
                           '--agent-args',str(peer),'--method-id','synthetic'],
                          cwd=home,env={'HOME':str(home),'PATH':'/usr/bin:/bin'},
                          stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=90)
    elapsed=time.monotonic()-started
    report={'successful':result.returncode==0 and 65<=elapsed<90,'delayedResponseSeconds':65,
            'elapsedSeconds':round(elapsed,1),'realAccountUsed':False,'modelTaskSubmitted':False}
    args.output.write_text(json.dumps(report)+'\n')
    print(json.dumps(report))
    raise SystemExit(0 if report['successful'] else 1)
