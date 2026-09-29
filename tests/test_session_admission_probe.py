import json
from pathlib import Path
import sys
import tempfile
import unittest
from session_admission_probe import client_reply, run
from vendor_adapter_discovery import ProbeFailure

PEER = '''import json,sys
r=json.loads(input());assert r['method']=='initialize'
print(json.dumps({'jsonrpc':'2.0','id':1,'result':{'authMethods':[{'id':'chat-gpt'}], 'agentCapabilities':{'sessionCapabilities':{},'mcpCapabilities':{'http':False}}}}),flush=True)
r=json.loads(input());assert r['method']=='authenticate' and r['params']['methodId']=='api-key'
print(json.dumps({'jsonrpc':'2.0','id':2,'error':{'code':-32600,'message':'subscription_policy_denied'}}),flush=True)
r=json.loads(input());assert r['method']=='session/new' and r['params']['mcpServers']==[]
print(json.dumps({'jsonrpc':'2.0','id':'p','method':'session/request_permission','params':{}}),flush=True)
r=json.loads(input());assert r['result']['outcome']['outcome']=='cancelled'
print(json.dumps({'jsonrpc':'2.0','id':3,'result':{'sessionId':'fixture'}}),flush=True)
'''

class Admission(unittest.TestCase):
    def test_no_prompt_and_permission_always_cancelled(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'peer.py';p.write_text(PEER)
            report={'clientRequestsDenied':0}
            run([sys.executable,str(p)],{},Path(d),'codex',report,timeout=3)
            self.assertTrue(report['sessionCreated'])
            self.assertTrue(report['apiAuthenticationRejected'])
            self.assertEqual(report['clientRequestsDenied'],1)

    def test_unknown_client_request_is_denied(self):
        reply=client_reply({'id':1,'method':'fs/write_text_file'})
        self.assertEqual(reply['error']['code'],-32601)
        self.assertNotIn('result',reply)

    def test_timeout_cleans_up_peer(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaisesRegex(ProbeFailure,'admission_timeout'):
                run([sys.executable,'-c','import time;time.sleep(10)'],{},Path(d),'codex',{},timeout=.05)

if __name__=='__main__':
    unittest.main()
