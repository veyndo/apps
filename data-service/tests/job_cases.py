"""Actual independent HTTP/MCP/SDK supervision; synthetic packages/jobs come only from the separate trusted fixture producer, never a public seed shortcut."""
import copy
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor

def run(g):
    sql,literal,request,rpc=(g[n] for n in ('sql','literal','request','rpc'))
    root,role,env=(g[n] for n in ('ROOT','ROLE','env'))
    def stop():
        if g['running']:
            p,log=g['running'];p.terminate();p.wait();log.close();g['running']=None
    assert 'apps.jobs.list' not in [t['name'] for t in rpc('tools/list')['result']['tools']]
    assert request('/v1/jobs/list',dict(app='app_jobs'))[0]==403
    stop()
    result=subprocess.run([g['PSQL'],g['DB_ADMIN'],'-X','-v','ON_ERROR_STOP=1','-v','reader_role='+role,'-f',str(root/'data-service/sql/enable-jobs.sql')],capture_output=True,text=True)
    assert result.returncode==0,result.stderr
    env=dict(env,APP_DATA_JOBS='enabled')
    g['start'](dict(env,APP_DATA_JOBS='disabled'),success=False)
    sql(f'GRANT UPDATE(result) ON app_jobs TO {role}');g['start'](env,success=False);sql(f'REVOKE UPDATE(result) ON app_jobs FROM {role}')
    sql(f'REVOKE DELETE ON app_jobs FROM {role}');g['start'](env,success=False);sql(f'GRANT DELETE ON app_jobs TO {role}')
    g['running']=g['start'](env)
    claims=g['CLAIMS'][g['TOKEN_A']];original=claims.copy()
    claims.update(scope='apps.read apps.manage team.manage catalog.read catalog.write b2b.read',personal_session=True,live=True)
    g['CLAIMS'][g['TOKEN_B']].update(claims,sub='user_b',tenant='shop_b')
    m=dict(id='app_jobs',version='0.1.0',coreApi='1',runtime='service',name={'en':'Long tasks','de':'Lange Aufgaben','es':'Tareas largas'},permissions=['jobs.read','jobs.write'],actions=[dict(name='prepare',description='Prepare the admitted task',handler='job',permission='catalog.read',requiredPermissions=['b2b.read'],inputSchema={'type':'object','additionalProperties':False,'required':['jobGroup'],'properties':{'jobGroup':{'type':'string'}}})])
    # These are pre-existing, trusted producer-owned job packages; the portable installer does not accept worker/service packages.
    for tenant in ['shop_a','shop_b']:
        sql(f"INSERT INTO app_packages(tenant,id,version,manifest,active) VALUES('{tenant}','app_jobs','0.1.0',{literal(m)},true)")
    def call(op,body,expected=200,token=None):
        status,value,headers=request('/v1/jobs/'+op,body,token=token or g['TOKEN_A']);assert status==expected,(op,status,value);assert headers['Cache-Control']=='no-store';return value
    def page(**query):return call('list',dict(app='app_jobs',**query))
    digest=page()['packageDigest']
    def seed(id,status='queued',actor='user_a',tenant='shop_a',package_digest=digest,fields=None):
        fields=fields if fields is not None else {'jobGroup':'batch'}
        sql(f"INSERT INTO app_jobs(tenant,app,id,action,actor,permission,package_digest,request_key,input_digest,input,status,message,result) VALUES('{tenant}','app_jobs','{id}','prepare','{actor}','catalog.read','{package_digest}','{id}','synthetic',{literal(fields)},'{status}',{literal({'en':'Prepared','de':'Vorbereitet','es':'Preparado'})},{literal({'secretResult':'own-'+tenant})})")
    def detail(id,expected=200,token=None):return call('detail',dict(app='app_jobs',id=id),expected,token)
    def control(id,op='cancel',revision=1,expected=200,**changes):
        v=dict(app='app_jobs',id=id,operation=op,revision=revision,approve=True,packageDigest=page()['packageDigest']);v.update(changes);return call('control',v,expected)
    for id,status in [('queued','queued'),('done','succeeded'),('failed','failed'),('running','running'),('uncertain','uncertain'),('race','queued'),('job_sdk','queued')]:seed(id,status)
    seed('queued',actor='user_b',tenant='shop_b');seed('other',actor='user_other');seed('stale','succeeded',package_digest='0'*64)
    sql("UPDATE app_jobs SET lease='original-lease',lease_until=now()+interval '60 seconds',started_at=now() WHERE tenant='shop_a' AND id='running'")
    rows=page(limit=2);assert len(rows['jobs'])==2 and rows['hasMore']
    assert page(after=rows['nextCursor'],limit=2)['jobs'][0]['id'] not in [r['id'] for r in rows['jobs']]
    assert len(page(group='batch')['jobs'])==9
    assert page(group="x' OR 1=1 --")['jobs']==[]
    call('list',dict(app='app_jobs',after='missing'),409)
    for body in [dict(app='app_jobs',limit=0),dict(app='app_jobs',limit=None),dict(app='app_jobs',id='queued'),dict(app='app_jobs',tenant='shop_b')]:call('list',body,400)
    done=detail('done');assert done['result']=={'secretResult':'own-shop_a'} and 'lease' not in done and 'input' not in done
    detail('stale',409);stale=next(r for r in page()['jobs'] if r['id']=='stale');assert stale['message']=={} and stale['result'] is None and 'packageDigest' not in stale
    claims['scope']='apps.read catalog.read b2b.read'
    detail('queued');detail('other',404);call('list',dict(app='app_jobs'),403)
    claims['scope']='apps.read apps.manage team.manage catalog.read'
    detail('done',403);assert all(r['result'] is None and r['message']=={} for r in page()['jobs'])
    claims['scope']='apps.read apps.manage team.manage catalog.read catalog.write b2b.read'
    for change in [dict(personal_session=False),dict(live=False),dict(scope='apps.read apps.manage catalog.read b2b.read')]:
        old=claims.copy();claims.update(change);control('queued',expected=403);claims.clear();claims.update(old)
    control('queued',approve=False,expected=400);control('queued',packageDigest='0'*64,expected=409)
    control('queued',tenant='shop_b',expected=400);control('queued',revision=None,expected=400)
    control('queued',op='complete',expected=400);control('queued',approveUnknownOutcome=None,expected=400)
    control('queued');assert detail('queued')['status']=='cancelled';control('queued',expected=409)
    assert detail('queued',token=g['TOKEN_B'])['status']=='queued'
    control('running');assert detail('running')['status']=='cancel_requested'
    assert sql("SELECT lease FROM app_jobs WHERE tenant='shop_a' AND id='running'")=='original-lease'
    control('failed','retry',expected=409)
    claims['scope']='apps.read apps.manage team.manage catalog.write b2b.read';control('failed','retry',approveUnknownOutcome=True,expected=403)
    claims['scope']='apps.read apps.manage team.manage catalog.read catalog.write b2b.read'
    sql("UPDATE app_jobs SET input='{}' WHERE tenant='shop_a' AND id='failed'")
    control('failed','retry',approveUnknownOutcome=True,expected=400)
    sql(f"UPDATE app_jobs SET input={literal({'jobGroup':'batch'})} WHERE tenant='shop_a' AND id='failed'")
    control('failed','retry',approveUnknownOutcome=True);assert detail('failed')['status']=='queued'
    assert sql("SELECT actor||'|'||package_digest FROM app_jobs WHERE tenant='shop_a' AND id='failed'")=='user_a|'+digest
    control('uncertain','resolve_cancelled',expected=409);control('uncertain','resolve_cancelled',approveUnknownOutcome=True);assert detail('uncertain')['status']=='cancelled'
    seed('expired','running');sql("UPDATE app_jobs SET lease_until=now()-interval '1 second' WHERE tenant='shop_a' AND id='expired'")
    assert detail('expired')['status']=='uncertain';control('expired','retry',approveUnknownOutcome=True);assert detail('expired')['status']=='queued'
    v=dict(app='app_jobs',id='race',operation='cancel',revision=1,approve=True,packageDigest=digest)
    with ThreadPoolExecutor(max_workers=2) as pool:assert sorted(pool.map(lambda _:request('/v1/jobs/control',v)[0],range(2)))==[200,409]
    # Real late outbox failure restores both row update and archival deletion.
    sql("ALTER TABLE outbox ADD CONSTRAINT fail_control CHECK(kind<>'app.job_controlled') NOT VALID")
    control('done','archive',expected=503);assert detail('done')['revision']==1
    seed('rollback');control('rollback',expected=503);assert detail('rollback')['status']=='queued'
    sql('ALTER TABLE outbox DROP CONSTRAINT fail_control');control('done','archive');detail('done',404)
    control('failed','archive',revision=2,expected=409)
    seed('overflow');sql("UPDATE app_jobs SET revision=9223372036854775807 WHERE tenant='shop_a' AND id='overflow'")
    control('overflow',revision=9223372036854775807,expected=503);assert detail('overflow')['status']=='queued'
    # Actual serving login has no producer/progress/result/identity/schema authority and cannot delete active work.
    prefix=f"BEGIN; SELECT set_config('veyndo.scope_ticket','{g['ticket']('shop_a')}',true); "
    for statement in ["UPDATE app_jobs SET result='{}'", "UPDATE app_jobs SET input='{}'", "UPDATE app_jobs SET id='forged'", "INSERT INTO app_jobs(tenant,app,id) VALUES('shop_a','app_jobs','forged')",'CREATE TABLE public.forbidden(id text)']:
        sql(prefix+statement+';ROLLBACK',g['READ_URL'],ok=False)
    sql(prefix+"DELETE FROM app_jobs WHERE id='failed';COMMIT",g['READ_URL']);assert detail('failed')['status']=='queued'
    assert sql(prefix+"SELECT count(*) FROM app_jobs WHERE tenant='shop_b';ROLLBACK",g['READ_URL']).splitlines()[-2]=='0'
    sql("UPDATE app_packages SET active=false WHERE tenant='shop_a' AND id='app_jobs'");detail('queued',404);control('failed',expected=404);assert page()['jobs']
    sql("UPDATE app_packages SET active=true WHERE tenant='shop_a' AND id='app_jobs'")
    names=[t['name'] for t in rpc('tools/list')['result']['tools']];assert all('apps.jobs.'+op in names for op in ['list','detail','control'])
    assert rpc('tools/call',dict(name='apps.jobs.detail',arguments=dict(app='app_jobs',id='queued')))['result']['structuredContent']==detail('queued')
    subprocess.run([g['os'].environ.get('NODE','node'),str(root/'data-service/examples/jobs.mjs')],env=dict(g['os'].environ,APP_DATA_URL=g['BASE'],APP_DATA_TOKEN=g['TOKEN_A'],APP_DATA_SDK_MODULE=g['os'].environ['APP_DATA_SDK_MODULE'],APP_JOB_ID='job_sdk'),check=True)
    stop();g['running']=g['start'](env);assert detail('job_sdk')['status']=='cancelled'
    assert request('/v1/contract')[1]['version']==5
    g['env']=env
    print('PASS independent job supervision: real restricted role, current authority, own/foreign shops and actors, bounded paging, result redaction, cancel/lease preservation, reviewed retry/resolve, archive, CAS races, full rollback, SDK/MCP and restart')
