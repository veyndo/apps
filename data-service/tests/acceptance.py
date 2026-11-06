#!/usr/bin/env python3
"""Actual Rust HTTP/MCP/SDK + independent PostgreSQL schema under a nonowner reader; all identities and records are synthetic."""
import hashlib
import hmac
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
ROOT = Path(__file__).resolve().parents[2]
PSQL = os.environ.get('PSQL', 'psql')
ADMIN = os.environ['APP_DATA_TEST_ADMIN_URL']
SUFFIX = uuid.uuid4().hex[:10]
DB, ROLE = 'app_reader_test_' + SUFFIX, 'app_reader_' + SUFFIX
PASSWORD = 'synthetic-read-only-' + SUFFIX
URL = urllib.parse.urlsplit(ADMIN)
DB_ADMIN = urllib.parse.urlunsplit(URL._replace(path='/' + DB))
READ_URL = urllib.parse.urlunsplit(URL._replace(netloc=f'{ROLE}:{PASSWORD}@{URL.hostname}:{URL.port or 5432}', path='/' + DB))
TOKEN_A, TOKEN_B = 'fixture-access-shop-a', 'fixture-access-shop-b'
CLAIMS = {TOKEN_A:dict(active=True, sub='user_a',tenant='shop_a',scope='apps.read catalog.read',aud='data-test',exp=int(time.time())+3600,merchant=True),TOKEN_B:dict(active=True, sub='user_b',tenant='shop_b',scope='apps.read catalog.read',aud='data-test',exp=int(time.time())+3600,merchant=True)}
CALLS = []
def sql(statement, url=DB_ADMIN, ok=True):
    result = subprocess.run([PSQL,url,'-XAt','-v','ON_ERROR_STOP=1'],input=statement,text=True,capture_output=True)
    assert (result.returncode == 0)==ok, 'Unexpected SQL acceptance/rejection: ' + result.stderr[:400]
    return result.stdout.strip()
def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1',0));return s.getsockname()[1]
class Identity(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_POST(self):
        data=urllib.parse.parse_qs(self.rfile.read(int(self.headers['Content-Length'])).decode())
        assert self.path=='/introspect'
        assert self.headers.get('Authorization')=='Bearer '+('i'*32)
        assert data['resource']==['data-test']
        token=data['token'][0]; CALLS.append(token)
        self.send_response(200);self.end_headers()
        self.wfile.write(json.dumps(CLAIMS.get(token,dict(active=False))).encode())
def request(path='/v1/records/query', body=None, token=TOKEN_A, headers=None):
    hdr={'Content-Type':'application/json'}
    if token:hdr['Authorization']='Bearer '+token
    hdr.update(headers or {})
    req=urllib.request.Request(BASE+path,data=None if body is None else json.dumps(body).encode(),headers=hdr)
    try:
        with urllib.request.urlopen(req,timeout=10) as r:
            return r.status,json.loads(r.read()),r.headers
    except urllib.error.HTTPError as e:
        return e.code,json.loads(e.read()),e.headers
QUERY=dict(app='advice',action='list_notes',query={'limit':1})
def read(body=QUERY, expected=200, token=TOKEN_A):
    status,data,headers=request(body=body,token=token)
    assert status==expected,(status,data)
    assert headers['Cache-Control']=='no-store'
    return data
def rpc(method,params=None):
    body=dict(jsonrpc='2.0',id=1,method=method)
    if params is not None:body['params']=params
    status,data,_=request('/mcp',body,headers={'MCP-Protocol-Version':'2025-11-25'})
    assert status==200,(status,data)
    return data
MANIFEST=dict(id='advice',version='0.1.0',coreApi='1',runtime='declarative',name={'en':'Advice','de':'Beratung'},permissions=['data.read'],entities=[dict(name='notes',fields=[dict(name='title',kind='string'),dict(name='body',kind='string',translatable=True)])],actions=[dict(name='list_notes',description='Read notes',handler='list',entity='notes',permission='catalog.read',requiredPermissions=[],mcp=True,inputSchema={'type':'object','additionalProperties':False,'properties':{'limit':{'type':'integer','minimum':1,'maximum':100},'after':{'type':'string'},'id':{'type':'string'}}})])
def literal(v):return "'"+json.dumps(v).replace("'","''")+"'::jsonb"
def start(env,success=True):
    log=open('/tmp/'+DB+'-service.log','w+')
    command=[str(ROOT/'data-service/target/debug/veyndo-app-data-service')]
    if os.environ.get('APP_DATA_TEST_IMAGE'):
        command=['docker','run','--init','--rm','--network','host','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--tmpfs','/tmp:rw,noexec,nosuid,size=16m']
        for name in env:
            if name.startswith('APP_DATA_') and name not in ('APP_DATA_TEST_IMAGE','APP_DATA_TEST_ADMIN_URL','APP_DATA_SDK_MODULE'):
                command.extend(['--env',name])
        command.append(os.environ['APP_DATA_TEST_IMAGE'])
    process=subprocess.Popen(command,env=env,stdout=log,stderr=log)
    for _ in range(150):
        if process.poll() is not None:
            log.seek(0);content=log.read();log.close()
            assert not success,content
            assert 'restricted' in content or 'Unsupported' in content, content
            return None
        if success:
            try:
                if urllib.request.urlopen(BASE+'/health',timeout=.2).status==200:return process,log
            except (OSError,urllib.error.URLError):pass
        time.sleep(.05)
    process.terminate();process.wait();log.close();raise AssertionError('Service readiness failed')
identity=None; running=None
try:
    sql(f'CREATE DATABASE {DB}',url=ADMIN)
    sql(f"CREATE ROLE {ROLE} LOGIN PASSWORD '{PASSWORD}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION NOINHERIT",url=ADMIN)
    installed=subprocess.run([PSQL,DB_ADMIN,'-X','-v','ON_ERROR_STOP=1','-v','reader_role='+ROLE,'-f',str(ROOT/'data-service/sql/install.sql')],capture_output=True,text=True)
    assert installed.returncode==0,installed.stderr
    for tenant,tag in [('shop_a','Alpha'),('shop_b','Beta')]:
        sql(f"INSERT INTO app_packages VALUES('{tenant}','advice',{literal(MANIFEST)},true); INSERT INTO app_models VALUES('{tenant}','advice','notes','0.1.0',{literal(MANIFEST['entities'][0])});")
        for i in range(3):sql(f"INSERT INTO app_records(tenant,app,entity,id,revision,fields) VALUES('{tenant}','advice','notes','note_{i}',{i+1},{literal({'title':tag+str(i),'body':{'en':'Hello','de':'Hallo'},'undeclared':'must not leak'})})")
    key_id,key=sql("SELECT id||'|'||secret FROM veyndo_scope_key()").split('|')
    # An arbitrary rac.* setting does not authenticate SQL. No direct key access, DDL or DML.
    assert sql("BEGIN; SELECT set_config('rac.tenant','shop_b',true),set_config('rac.system','on',true); SELECT count(*) FROM app_records; ROLLBACK;",READ_URL).splitlines()[-2]=='0'
    sql('SELECT * FROM veyndo_scope_key()',READ_URL,ok=False)
    sql('CREATE TEMP TABLE forbidden(id text)',READ_URL,ok=False)
    sql('TRUNCATE app_records',READ_URL,ok=False)
    sql("INSERT INTO app_packages VALUES('shop_a','evil','{}',true)",READ_URL,ok=False)
    def ticket(tenant,role=ROLE,secret=key,deadline=None):
        body=f'v1:{key_id}:{deadline or int(time.time())+120}:A:{tenant}:user_a:{uuid.uuid4().hex}'
        return body+':'+hmac.new(bytes.fromhex(secret),(body+':'+role).encode(),hashlib.sha256).hexdigest()
    def count(t):
        return sql(f"BEGIN; SELECT set_config('veyndo.scope_ticket','{t}',true); SELECT count(*) FROM app_records; ROLLBACK;",READ_URL).splitlines()[-2]
    assert count(ticket('shop_a'))=='3'
    assert count(ticket('shop_b',role='wrong_role'))=='0'
    assert count(ticket('shop_b',secret='00'*32))=='0'
    assert count(ticket('shop_b',deadline=int(time.time())-20))=='0'
    assert count(ticket('shop_b').replace(':shop_b:',':shop_a:'))=='0'
    identity=http.server.ThreadingHTTPServer(('127.0.0.1',0),Identity)
    threading.Thread(target=identity.serve_forever,daemon=True).start()
    BASE='http://127.0.0.1:'+str(port())
    env=dict(os.environ,APP_DATA_BIND=BASE.removeprefix('http://'),APP_DATA_INTROSPECTION_URL=f'http://127.0.0.1:{identity.server_port}/introspect',APP_DATA_INTROSPECTION_TOKEN='i'*32,APP_DATA_AUDIENCE='data-test',APP_DATA_DATABASE_URL=READ_URL,APP_DATA_DATABASE_ROLE=ROLE,APP_DATA_SCOPE_KEY_ID=key_id,APP_DATA_SCOPE_KEY=key)
    running=start(env)
    first=read();assert first['elements'][0]['title']=='Alpha0' and first['hasMore'] and first['nextCursor']=='note_0'
    assert 'undeclared' not in first['elements'][0]
    assert first['elements'][0]['body']=={'en':'Hello','de':'Hallo'}
    assert read(dict(QUERY,query={'after':first['nextCursor'],'limit':1}))['elements'][0]['title']=='Alpha1'
    assert read(dict(QUERY,query={}))['limit']==100
    assert read(token=TOKEN_B)['elements'][0]['title']=='Beta0'
    assert read(dict(QUERY,query={'id':'note_2'}))['elements'][0]['revision']==3
    read(expected=401,token=None)
    for body in [dict(QUERY,tenant='shop_b'),dict(QUERY,query={'limit':101}),dict(QUERY,query={'limit':0}),dict(QUERY,query={'filter':{'title':'Beta0'}}),dict(QUERY,query={'id':''}),dict(QUERY,query={'limit':None}),dict(QUERY,query={'after':None}),dict(QUERY,query={'id':None})]:read(body,400)
    read(dict(QUERY,app='not_installed'),404)
    for change in [dict(scope='apps.read'),dict(merchant=False),dict(active=False),dict(aud='other'),dict(exp=1)]:
        old=CLAIMS[TOKEN_A].copy();CLAIMS[TOKEN_A].update(change)
        read(expected=401 if any(k in change for k in ['active','aud','exp']) else 403)
        CLAIMS[TOKEN_A]=old
    assert len(CALLS)>10
    assert rpc('initialize',{'protocolVersion':'2025-11-25'})['result']['serverInfo']['name']=='veyndo-app-data'
    assert rpc('tools/list')['result']['tools'][0]['name']=='apps.records.read'
    assert rpc('tools/call',dict(name='apps.records.read',arguments=QUERY))['result']['structuredContent']==first
    sql("UPDATE app_packages SET manifest=jsonb_set(manifest,'{actions,0,mcp}','false') WHERE tenant='shop_a'")
    assert rpc('tools/call',dict(name='apps.records.read',arguments=QUERY))['result']['isError']
    assert read()['elements'][0]['title']=='Alpha0'
    sql("UPDATE app_packages SET active=false WHERE tenant='shop_a'");read(expected=404)
    assert read(token=TOKEN_B)['elements'][0]['title']=='Beta0'
    sql("UPDATE app_packages SET active=true,manifest=jsonb_set(manifest,'{actions,0,mcp}','true') WHERE tenant='shop_a'")
    sql("UPDATE app_packages SET manifest=jsonb_set(manifest,'{entities,0,fields,0,coreReference}','\"product\"') WHERE tenant='shop_a'")
    read(expected=422)
    sql(f"UPDATE app_packages SET manifest={literal(MANIFEST)} WHERE tenant='shop_a'")
    # Existing generic SDK, no special native Core/server: actual HTTP and MCP on this reader.
    sdk=os.environ['APP_DATA_SDK_MODULE']
    subprocess.run([os.environ.get('NODE','node'),str(ROOT/'data-service/examples/sdk.mjs')],env=dict(os.environ,APP_DATA_URL=BASE,APP_DATA_TOKEN=TOKEN_A,APP_DATA_SDK_MODULE=sdk),check=True)
    running[0].terminate();running[0].wait();running[1].close();running=None
    # Startup refuses owner/superuser configuration even with otherwise valid keys.
    start(dict(env,APP_DATA_DATABASE_URL=DB_ADMIN),success=False)
    # Wrong signing key fails closed at the real SQL verifier, never returns an empty successful page.
    running=start(dict(env,APP_DATA_SCOPE_KEY='00'*32));read(expected=503)
    print('PASS independent Rust HTTP/MCP/SDK, two shops, signed RLS, restricted reader, current rights, revocation, package disable, projection, pagination and native-resolver denial')
finally:
    if running:running[0].terminate();running[0].wait();running[1].close()
    if identity:identity.shutdown();identity.server_close()
    sql(f'DROP DATABASE IF EXISTS {DB} WITH (FORCE)',ADMIN)
    sql(f'DROP ROLE IF EXISTS {ROLE}',ADMIN)
