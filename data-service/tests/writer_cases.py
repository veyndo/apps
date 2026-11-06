"""Managed-write extension of the real reader process fixture; no Core process, direct privileged API or assumed permissions."""
import copy
import hashlib
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor

def run(g):
    sql, literal, request, read, rpc = (g[n] for n in ('sql','literal','request','read','rpc'))
    root, env, role = (g[n] for n in ('ROOT','env','ROLE'))
    def stop():
        if g['running']:
            process,log=g['running'];process.terminate();process.wait();log.close();g['running']=None
    assert 'apps.records.write' not in [t['name'] for t in rpc('tools/list')['result']['tools']]
    status,_,_=request('/v1/records/write',dict(app='advice',action='save',packageDigest='0'*64,approve=True,input=dict(id='disabled',revision=0,fields={})))
    assert status==403
    stop()
    upgraded=subprocess.run([g['PSQL'],g['DB_ADMIN'],'-X','-v','ON_ERROR_STOP=1','-v','reader_role='+role,'-f',str(root/'data-service/sql/enable-writes.sql')],capture_output=True,text=True)
    assert upgraded.returncode==0,upgraded.stderr
    schema=g['MANIFEST']['actions'][0]['inputSchema']
    manifest=dict(id='cases',version='0.1.0',coreApi='1',runtime='declarative',name={'en':'Cases','de':'Vorgänge'},permissions=['data.read','data.write'],entities=[dict(name='cases',fields=[dict(name='title',kind='string',required=True),dict(name='reference',kind='string',required=True),dict(name='status',kind='string',required=True,choices=[dict(value='new',label={'en':'New'}),dict(value='closed',label={'en':'Closed'})])],workflow=dict(field='status',initial='new',immutableFields=['reference'],transitions=[dict(from_='unused')]))],actions=[dict(name='list_cases',description='Read cases',handler='list',entity='cases',permission='catalog.read',mcp=True,inputSchema=schema),dict(name='save_case',description='Save case',handler='save',entity='cases',permission='catalog.write',mcp=True,inputSchema={'type':'object','required':['id','revision','fields'],'additionalProperties':False,'properties':{'id':{'type':'string'},'revision':{'type':'integer'},'fields':{'type':'object'}}})])
    manifest['entities'][0]['workflow']['transitions']=[{'from':'new','to':'closed'}]
    for tenant in ('shop_a','shop_b'):
        sql(f"INSERT INTO app_packages VALUES('{tenant}','cases',{literal(manifest)},true); INSERT INTO app_models VALUES('{tenant}','cases','cases','0.1.0',{literal(manifest['entities'][0])});")
    env=dict(env,APP_DATA_MODE='write')
    # Read profile never accepts silently expanded database privileges.
    g['start'](dict(env,APP_DATA_MODE='read'),success=False)
    # Writer cannot start if a dangerous new grant is present.
    sql(f'GRANT DELETE ON app_records TO {role}')
    g['start'](env,success=False)
    sql(f'REVOKE DELETE ON app_records FROM {role}')
    g['running']=g['start'](env)
    claims=g['CLAIMS'][g['TOKEN_A']]
    claims['scope']='apps.read apps.manage catalog.read catalog.write'
    def page():
        return read(dict(app='cases',action='list_cases',query={}))
    def query(record):
        return read(dict(app='cases',action='list_cases',query={'id':record}))
    def change(record='case_1',revision=0,status='new',title='First',reference='stable'):
        return dict(app='cases',action='save_case',packageDigest=page()['packageDigest'],approve=True,input=dict(id=record,revision=revision,fields=dict(title=title,reference=reference,status=status)))
    def write(body,expected=200,token=None):
        status,value,_=request('/v1/records/write',body,token=token or g['TOKEN_A']);assert status==expected,(status,value);return value
    def history(record='case_1'):
        status,value,_=request('/v1/records/history',dict(app='cases',action='list_cases',id=record));assert status==200,(status,value);return value['versions']
    assert 'apps.records.write' in [t['name'] for t in rpc('tools/list')['result']['tools']]
    assert write(change())['revision']==1
    assert history()[0]['actor']=='user_a' and history()[0]['source']=='app.save'
    assert sql("SELECT count(*) FROM outbox WHERE tenant='shop_a'")=='1'
    assert sql("SELECT data ? 'fields' FROM outbox LIMIT 1")=='f'
    write(change(revision=0),409)
    write(dict(change(revision=1),approve=False),400)
    write(dict(change(revision=1),packageDigest='0'*64),409)
    write(change(revision=1,status='new',reference='changed'),409)
    write(change(revision=1,status='unknown'),400)
    write(change(revision=-1),400)
    write(change(title='x'*131072),413)
    bad=change(revision=1);bad['input']['fields']['unknown']='private';write(bad,400)
    old=claims['scope'];claims['scope']='apps.read catalog.read catalog.write';write(change(revision=1),403);claims['scope']=old
    write(change(revision=1),403,token=g['TOKEN_B'])
    assert query('case_1')['elements'][0]['revision']==1 and len(history())==1
    # Native states are unchanged; a foreign tenant's ID never edits the victim.
    other=dict(change('case_1',0),packageDigest=page()['packageDigest'])
    g['CLAIMS'][g['TOKEN_B']]['scope']=old
    assert write(other,token=g['TOKEN_B'])['revision']==1
    assert query('case_1')['elements'][0]['revision']==1
    # Two concurrent writes, one winner, one event and one new version.
    competing=change(revision=1,status='closed')
    def race(_):return request('/v1/records/write',competing)[0]
    with ThreadPoolExecutor(max_workers=2) as pool:assert sorted(pool.map(race,range(2)))==[200,409]
    assert [v['revision'] for v in history()]==[2,1]
    assert sql("SELECT count(*) FROM outbox WHERE tenant='shop_a'")=='2'
    write(change(revision=2,status='new'),409)
    # MCP cannot invoke an opted-out action; ordinary HTTP uses its independent rights.
    sql("UPDATE app_packages SET manifest=jsonb_set(manifest,'{actions,1,mcp}','false') WHERE tenant='shop_a' AND id='cases'")
    assert rpc('tools/call',dict(name='apps.records.write',arguments=change('http_only')))['result']['isError']
    assert write(change('http_only'))['revision']==1
    sql(f"UPDATE app_packages SET manifest={literal(manifest)} WHERE tenant='shop_a' AND id='cases'")
    # A stale reviewed package cannot be used after deactivation.
    prepared=change('disabled')
    sql("UPDATE app_packages SET active=false WHERE tenant='shop_a' AND id='cases'")
    write(prepared,409)
    sql("UPDATE app_packages SET active=true WHERE tenant='shop_a' AND id='cases'")
    # Late event and output validation failures roll back data, history and event.
    sql("ALTER TABLE outbox ADD CONSTRAINT reject_event CHECK(data->>'id'<>'fail_event')")
    write(change('fail_event'),503)
    assert query('fail_event')['elements']==[] and history('fail_event')==[]
    sql('ALTER TABLE outbox DROP CONSTRAINT reject_event')
    sql("ALTER TABLE app_record_versions ADD CONSTRAINT reject_history CHECK(fields->>'title'<>'late_fail')")
    write(change('fail_history',title='late_fail'),503)
    assert query('fail_history')['elements']==[] and history('fail_history')==[]
    sql('ALTER TABLE app_record_versions DROP CONSTRAINT reject_history')
    invalid=copy.deepcopy(manifest);invalid['actions'][1]['outputSchema']={'type':'object','properties':{'revision':{'type':'integer','maximum':0}}}
    sql(f"UPDATE app_packages SET manifest={literal(invalid)} WHERE tenant='shop_a' AND id='cases'")
    write(change('invalid_output'),422);assert query('invalid_output')['elements']==[]
    sql(f"UPDATE app_packages SET manifest={literal(manifest)} WHERE tenant='shop_a' AND id='cases'")
    # Package row locks never grant the serving login actual package edits.
    ticket=g['ticket']('shop_a')
    sql(f"BEGIN; SELECT set_config('veyndo.scope_ticket','{ticket}',true); UPDATE app_packages SET active=false WHERE tenant='shop_a' AND id='cases'; ROLLBACK;",g['READ_URL'],ok=False)
    sql(f"BEGIN; SELECT set_config('veyndo.scope_ticket','{ticket}',true); UPDATE app_records SET tenant='shop_b' WHERE tenant='shop_a'; ROLLBACK;",g['READ_URL'],ok=False)
    # Native bound/asset/index/translatable models have no permissive fallback.
    for field in ['translatable','indexed','unique','coreReference']:
        unsupported=copy.deepcopy(manifest);unsupported['entities'][0]['fields'][0][field]='product' if field=='coreReference' else True
        prepared=change('unsupported')
        prepared['packageDigest']=hashlib.sha256(json.dumps(unsupported,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
        sql(f"UPDATE app_packages SET manifest={literal(unsupported)} WHERE tenant='shop_a' AND id='cases'")
        write(prepared,422)
        sql(f"UPDATE app_packages SET manifest={literal(manifest)} WHERE tenant='shop_a' AND id='cases'")
    sql(f"UPDATE app_packages SET manifest={literal(manifest)} WHERE tenant='shop_a' AND id='cases'")
    # Original read rights remain bound to a captured version even after the manifest narrows them.
    guarded=copy.deepcopy(manifest);guarded['actions'][0]['requiredPermissions']=['b2b.read']
    sql(f"UPDATE app_packages SET manifest={literal(guarded)} WHERE tenant='shop_a' AND id='cases'")
    claims['scope']=old+' b2b.read';write(change('historical_private'))
    sql(f"UPDATE app_packages SET manifest={literal(manifest)} WHERE tenant='shop_a' AND id='cases'")
    claims['scope']=old
    assert history('historical_private')==[]
    # Retention is bounded per record; writes remain usable after reconnect.
    for rev in range(23):write(change('retention',revision=rev))
    assert len(history('retention'))==20
    stop();g['running']=g['start'](env)
    assert query('case_1')['elements'][0]['revision']==2
    # Same generic SDK executes the write and history tools and the existing read operation.
    subprocess.run([g['os'].environ.get('NODE','node'),str(root/'data-service/examples/write.mjs')],env=dict(g['os'].environ,APP_DATA_URL=g['BASE'],APP_DATA_TOKEN=g['TOKEN_A'],APP_DATA_SDK_MODULE=g['os'].environ['APP_DATA_SDK_MODULE']),check=True)
    print('PASS managed write profile: current scopes, package fences, CAS races, states, historical rights/retention, atomic rollback, SDK/MCP and reconnect')
