#!/usr/bin/env python3
"""Actual standalone Rust process, real local identity introspection, HTTP/MCP/JS parity and revocation without Core or PostgreSQL."""
import copy, hashlib, http.client, json, os, socket, subprocess, sys, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs
from urllib.request import Request, urlopen
from urllib.error import HTTPError
ROOT=Path(__file__).resolve().parents[2]
TOKEN='synthetic-alpha-access-00001'
BROKER='synthetic-introspection-client-'+'x'*32
calls=[]; revoked=set()
class Identity(BaseHTTPRequestHandler):
 def log_message(self,*_): pass
 def do_POST(self):
  assert self.path=='/introspect'
  assert self.headers['Authorization']=='Bearer '+BROKER
  assert self.headers.get_content_type()=='application/x-www-form-urlencoded'
  args=parse_qs(self.rfile.read(int(self.headers['Content-Length'])).decode())
  token=args['token'][0];calls.append(token)
  assert args['token_type_hint']==['access_token'] and args['resource']==['runtime-test']
  if token=='synthetic-backend-failure':
   self.send_response(500);self.end_headers();self.wfile.write(b'PRIVATE-IDENTITY-DETAIL');return
  if token=='synthetic-redirect-token':
   self.send_response(302);self.send_header('Location','/must-not-follow');self.end_headers();return
  if token=='synthetic-oversized-token':
   self.send_response(200);self.end_headers();self.wfile.write(b' '*17000);return
  data={'active':True,'sub':'actor-1','tenant':'alpha','aud':'runtime-test','scope':'apps.evaluate','exp':int(time.time())+3600}
  if token in revoked or token=='synthetic-inactive-token':data={'active':False}
  if token=='synthetic-no-scope-token':data['scope']='apps.evaluate.admin'
  if token=='synthetic-expired-token':data['exp']=int(time.time())-1
  if token=='synthetic-wrong-audience':data['aud']=['another-service']
  if token=='synthetic-no-tenant-token':del data['tenant']
  if token=='synthetic-array-audience':data['aud']=['another-service','runtime-test']
  if token=='synthetic-beta-access-0002':data.update(tenant='beta')
  self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps(data).encode())
def free_port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def run():
 identity=ThreadingHTTPServer(('127.0.0.1',0),Identity);threading.Thread(target=identity.serve_forever,daemon=True).start()
 port=free_port();base=f'http://127.0.0.1:{port}'
 env={**os.environ,'APP_RUNTIME_BIND':f'127.0.0.1:{port}','APP_RUNTIME_INTROSPECTION_URL':f'http://127.0.0.1:{identity.server_port}/introspect','APP_RUNTIME_INTROSPECTION_TOKEN':BROKER,'APP_RUNTIME_AUDIENCE':'runtime-test','APP_RUNTIME_ORIGINS':'["http://127.0.0.1:9000"]'}
 binary=Path(os.environ.get('APP_RUNTIME_TEST_BINARY',ROOT/'service/target/debug/veyndo-app-runtime-service')).resolve()
 with open(os.environ.get('APP_RUNTIME_TEST_LOG','/tmp/veyndo-app-runtime-service-test.log'),'w') as log:
  command=[str(binary)]
  image=os.environ.get('APP_RUNTIME_TEST_IMAGE')
  if image:command=['docker','run','--rm','--network','host',*[value for name in env if name.startswith('APP_RUNTIME_') and name not in ['APP_RUNTIME_TEST_IMAGE','APP_RUNTIME_SDK_MODULE'] for value in ['-e',name]],image]
  child=subprocess.Popen(command,env=env,stdout=log,stderr=log)
  def call(path,body=None,token=TOKEN,status=200,headers=None,raw=None):
   auth={} if token is None else {'Authorization':'Bearer '+token}
   req=Request(base+path,data=raw if raw is not None else None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json',**auth,**(headers or {})})
   try:
    with urlopen(req,timeout=15) as r:code=r.status;result=r.read();h=r.headers
   except HTTPError as e:code=e.code;result=e.read();h=e.headers
   assert code==status,(path,code,status,result)
   assert h['Cache-Control']=='no-store' and h['X-Content-Type-Options']=='nosniff',(path,h)
   assert TOKEN.encode() not in result and BROKER.encode() not in result and b'PRIVATE-IDENTITY-DETAIL' not in result
   return json.loads(result) if result else None
  try:
   for _ in range(200):
    if child.poll() is not None:raise AssertionError('Rust service exited during startup')
    try:call('/health',token=None);break
    except OSError:time.sleep(.05)
   else:raise AssertionError('Rust service failed to start')
   assert not calls
   call('/v1/contract',token=None,status=401)
   call('/v1/contract',token='synthetic-no-scope-token',status=403)
   for t in ['synthetic-inactive-token','synthetic-expired-token','synthetic-wrong-audience','synthetic-no-tenant-token']:call('/v1/contract',token=t,status=401)
   for t in ['synthetic-backend-failure','synthetic-redirect-token','synthetic-oversized-token']:call('/v1/contract',token=t,status=503)
   call('/v1/contract',token='synthetic-array-audience')
   count=len(calls)
   call('/v1/contract',headers={'Origin':'https://attacker.example'},status=403)
   assert len(calls)==count
   call('/v1/contract',headers={'Origin':'http://127.0.0.1:9000'})
   conn=http.client.HTTPConnection('127.0.0.1',port);conn.putrequest('GET','/v1/contract');conn.putheader('Authorization','Bearer '+TOKEN);conn.putheader('Authorization','Bearer '+TOKEN);conn.endheaders();r=conn.getresponse();assert r.status==401;r.read();conn.close()
   contract=call('/v1/contract');openapi=call('/v1/openapi.json');assert openapi['openapi']=='3.1.0'
   operation=openapi['paths']['/v1/evaluations']['post'];assert operation['requestBody']['content']['application/json']['schema']==contract['inputSchema'] and operation['responses']['200']['content']['application/json']['schema']==contract['outputSchema']
   assert contract['effects']==[] and contract['version']==1
   assert contract['mcp']['tool']['inputSchema']==contract['inputSchema']
   source=(ROOT/'runtime/tests/fixtures/snapshot-price.component.wat').read_text()
   input={'source':source,'hook':'price','snapshot':{'subtotal':{'minor':25000,'currency':'EUR','scale':2}},'context':{}}
   output=call('/v1/evaluations',input)
   assert output['mode']=='simulation' and output['effectApplied'] is False and output['contractVersion']==1
   assert output['outcome']=={'adjustment':{'minor':250,'currency':'EUR','scale':2},'accepted':True,'reasonCode':'snapshot_price'},output
   assert output['sourceDigest']==hashlib.sha256(source.encode()).hexdigest() and len(output['inputDigest'])==64
   for kind in ['shipping','discount','validation']:
    value=copy.deepcopy(input);value['hook']=kind
    result=call('/v1/evaluations',value);assert result['outcome']['adjustment']['minor']==(0 if kind=='validation' else 250)
   def rpc(method,params=None,status=200,headers=None):
    value={'jsonrpc':'2.0','id':'same','method':method}
    if params is not None:value['params']=params
    return call('/mcp',value,status=status,headers=headers)
   assert rpc('initialize',{'protocolVersion':'2025-06-18'})['result']['protocolVersion']=='2025-06-18'
   assert rpc('ping')['result']=={}
   tools=rpc('tools/list')['result']['tools'];assert tools==[contract['mcp']['tool']]
   mcp=rpc('tools/call',{'name':'apps.evaluate','arguments':input})['result'];assert not mcp['isError'] and mcp['structuredContent']==output
   assert json.loads(mcp['content'][0]['text'])==output
   assert rpc('tools/call',{'name':'apps.write','arguments':input})['error']['code']==-32602
   assert rpc('missing')['error']['code']==-32601
   rpc('ping',headers={'MCP-Protocol-Version':'not-supported'},status=400)
   call('/mcp',{'jsonrpc':'2.0','method':'notifications/initialized'},status=202)
   no_id=call('/mcp',{'jsonrpc':'2.0','method':'tools/call','params':{'name':'apps.evaluate','arguments':input}},status=400);assert no_id['error']['code']==-32600
   for key,value in [('tenant','beta'),('permissions',['apps.manage']),('principal',{'role':'owner'}),('namespaces','legacy')]:
    bad=copy.deepcopy(input);bad[key]=value;call('/v1/evaluations',bad,status=400)
   bad_inputs=[]
   for value in [-1,9007199254740992]:
    v=copy.deepcopy(input);v['snapshot']['subtotal']['minor']=value;bad_inputs.append(v)
   v=copy.deepcopy(input);v['context']='not an object';bad_inputs.append(v)
   v=copy.deepcopy(input);v['context']={'big':'x'*8192};bad_inputs.append(v)
   v=copy.deepcopy(input);v['source']='x'*32769;bad_inputs.append(v)
   v=copy.deepcopy(input);v['snapshot']['products']=[{'id':'same','stock':1,'price':v['snapshot']['subtotal']}]*2;bad_inputs.append(v)
   v=copy.deepcopy(input);v['snapshot']['records']=[{'entity':'secret','id':'id','revision':0,'fields':{}}];bad_inputs.append(v)
   for v in bad_inputs:call('/v1/evaluations',v,status=400)
   for source_bad in ['(component)','(module)',source.replace('veyndo:commerce','evil:commerce'),source.replace('hooks@1.0.0','hooks@2.0.0')]:
    v=copy.deepcopy(input);v['source']=source_bad;call('/v1/evaluations',v,status=422)
    assert rpc('tools/call',{'name':'apps.evaluate','arguments':v})['result']['isError']
   call('/v1/evaluations',raw=b'{"not":"json"',status=400)
   call('/v1/evaluations',raw=b'{"source":"'+b'x'*140000+b'"}',status=413)
   # Same current token is checked afresh after revocation; submitted tenant/rights cannot restore it.
   count=len(calls);revoked.add(TOKEN)
   call('/v1/evaluations',input,status=401);call('/mcp',{'jsonrpc':'2.0','id':1,'method':'tools/list'},status=401)
   assert len(calls)==count+2
   revoked.remove(TOKEN)
   assert call('/v1/evaluations',input,token='synthetic-beta-access-0002')==output
   # The public JS example uses both HTTP and MCP against this actual process.
   subprocess.run([os.environ.get('NODE','node'),str(ROOT/'service/examples/evaluate.mjs'),str(ROOT/'runtime/tests/fixtures/snapshot-price.component.wat')],env={**env,'APP_RUNTIME_URL':base,'APP_RUNTIME_TOKEN':TOKEN},check=True,stdout=subprocess.PIPE,text=True)
   sdk=os.environ.get('APP_RUNTIME_SDK_MODULE')
   assert sdk,'Set APP_RUNTIME_SDK_MODULE to the pinned veyndo/sdk src/index.js'
   subprocess.run([os.environ.get('NODE','node'),str(ROOT/'service/examples/sdk.mjs'),str(ROOT/'runtime/tests/fixtures/snapshot-price.component.wat')],env={**env,'APP_RUNTIME_URL':base,'APP_RUNTIME_TOKEN':TOKEN,'APP_RUNTIME_SDK_MODULE':sdk},check=True,stdout=subprocess.PIPE,text=True)
   assert child.poll() is None
   print('PASS independent Rust HTTP/MCP/JS: actual Wasm, same contract/output, fresh identity/revocation, origin/duplicate auth, typed money/snapshot/source limits, sanitized provider errors and no Core/database')
  finally:
   child.terminate()
   try:child.wait(timeout=5)
   except subprocess.TimeoutExpired:child.kill();child.wait()
   identity.shutdown();identity.server_close()
if __name__=='__main__':run()
