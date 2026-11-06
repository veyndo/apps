"""No Core or privileged seed endpoint: complete review/install/settings/record flow on the actual independent Rust host."""
import copy
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor

def run(g):
    sql,literal,request,rpc=(g[n] for n in ('sql','literal','request','rpc'))
    env,root,role=(g[n] for n in ('env','ROOT','ROLE'))
    def stop():
        if g['running']:
            p,log=g['running'];p.terminate();p.wait();log.close();g['running']=None
    assert 'apps.packages.install' not in [t['name'] for t in rpc('tools/list')['result']['tools']]
    assert request('/v1/packages/review',dict(manifest={}))[0]==403
    stop()
    migration=subprocess.run([g['PSQL'],g['DB_ADMIN'],'-X','-v','ON_ERROR_STOP=1','-v','reader_role='+role,'-f',str(root/'data-service/sql/enable-installation.sql')],text=True,capture_output=True)
    assert migration.returncode==0,migration.stderr
    env=dict(env,APP_DATA_INSTALLS='enabled',APP_DATA_INSTALL_SCOPES=json.dumps(['catalog.read','catalog.write','apps.read','apps.manage','team.manage','b2b.read']))
    g['start'](dict(env,APP_DATA_INSTALLS='disabled'),success=False)
    sql(f'GRANT UPDATE ON app_versions TO {role}')
    g['start'](env,success=False)
    sql(f'REVOKE UPDATE ON app_versions FROM {role}')
    g['running']=g['start'](env)
    claims=g['CLAIMS'][g['TOKEN_A']]
    claims.update(scope='apps.read apps.manage team.manage catalog.read catalog.write',personal_session=True,live=True)
    g['CLAIMS'][g['TOKEN_B']].update(claims,tenant='shop_b',sub='user_b')
    manifest=json.loads((root/'data-service/tests/portable-package.json').read_text())
    def call(path,body,expected=200,token=None):
        status,value,_=request(path,body,token=token or g['TOKEN_A']);assert status==expected,(path,status,value);return value
    def review(m=manifest,expected=200,token=None):return call('/v1/packages/review',dict(manifest=m),expected,token)
    def approved(m=manifest):
        r=review(m);return dict(manifest=m,digest=r['digest'],revision=r['revision'],permissions=r['permissions'],approve=True)
    def install(v,expected=200,token=None):return call('/v1/packages/install',v,expected,token)
    first=review();assert first['revision']==0 and first['previousVersion'] is None and first['name']['de']=='Notizen'
    assert sql("SELECT count(*) FROM app_packages WHERE id='app_notes'")=='0'
    proposal=approved()
    for alteration in [dict(personal_session=False),dict(live=False),dict(scope='apps.read apps.manage'),dict(scope='apps.read team.manage')]:
        original=claims.copy();claims.update(alteration);review(expected=403);install(proposal,403);claims.clear();claims.update(original)
    install(dict(proposal,approve=False),400)
    install(dict(proposal,digest='0'*64),400)
    install(dict(proposal,permissions=['data.read']),400)
    install(dict(proposal,tenant='shop_b'),400)
    for change in [dict(runtime='service'),dict(id='payments'),dict(id='site_builder'),dict(views=[{}]),dict(permissions=['data.read','service.call'])]:
        review(dict(manifest,**change),422 if 'views' not in change else 400)
    invalid=copy.deepcopy(manifest);invalid['entities'][0]['fields'][0]['indexed']=True;review(invalid,422)
    invalid=copy.deepcopy(manifest);invalid['actions'][0]['permission']='unknown.read';review(invalid,400)
    installed=install(proposal);assert installed['revision']==1 and installed['digest']==first['digest']
    assert sql("SELECT count(*) FROM app_versions WHERE tenant='shop_a' AND app='app_notes'")=='1'
    assert sql("SELECT count(*) FROM outbox WHERE tenant='shop_a' AND kind='app.installed'")=='1'
    install(proposal,409)
    # All following effects use the installed contract, never fixture SQL to create an App/model/digest.
    settings=call('/v1/settings/read',dict(app='app_notes'));assert settings['digest']==installed['digest'] and settings['revision']==0
    call('/v1/settings/save',dict(app='app_notes',digest=settings['digest'],revision=0,approve=True,values={'api_token':'synthetic-installed-secret'}))
    assert 'synthetic-installed-secret' not in json.dumps(call('/v1/settings/read',dict(app='app_notes')))
    def write(record='one',revision=0,title='First',token=None):
        digest=review()['digest']
        return call('/v1/records/write',dict(app='app_notes',action='save_note',packageDigest=digest,approve=True,input=dict(id=record,revision=revision,fields=dict(title=title,enabled=False))),token=token)
    def read(token=None):return call('/v1/records/query',dict(app='app_notes',action='list_notes',query={}),token=token)
    assert write()['revision']==1 and read()['elements'][0]['title']=='First'
    assert sql("SELECT rows FROM app_storage_usage WHERE tenant='shop_a' AND app='app_notes'")=='1'
    assert sql("SELECT version FROM app_records WHERE tenant='shop_a' AND app='app_notes'")=='0.1.0'
    assert request('/v1/records/query',dict(app='app_notes',action='list_notes',query={}),token=g['TOKEN_B'])[0]==404
    other=review(token=g['TOKEN_B']);assert other['revision']==0
    install(proposal,token=g['TOKEN_B']);assert read(token=g['TOKEN_B'])['elements']==[]
    assert read()['elements'][0]['title']=='First'
    race=dict(manifest,id='app_race');prepared=approved(race)
    with ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(lambda _:request('/v1/packages/install',prepared)[0],range(2)))==[200,409]
    assert sql("SELECT count(*) FROM app_versions WHERE tenant='shop_a' AND app='app_race'")=='1'
    upgrade=copy.deepcopy(manifest);upgrade['version']='0.2.0';upgrade['entities'][0]['fields'].append(dict(name='hint',kind='string'))
    r=install(approved(upgrade));assert r['revision']==2
    assert read()['elements'][0]['revision']==1 and read()['elements'][0]['title']=='First'
    assert sql("SELECT version FROM app_records WHERE tenant='shop_a' AND app='app_notes'")=='0.2.0'
    s=call('/v1/settings/read',dict(app='app_notes'));assert not s['current'] and s['configured']==[]
    changed=copy.deepcopy(upgrade);changed['name']['en']='Mutated same version';review(changed,409)
    review(dict(manifest,version='0.1.5'),409)
    removed=copy.deepcopy(upgrade);removed['version']='0.3.0';removed['entities'][0]['fields']=[f for f in removed['entities'][0]['fields'] if f['name']!='title'];review(removed,409)
    required=copy.deepcopy(upgrade);required['version']='0.3.0';required['entities'][0]['fields'].append(dict(name='required',kind='string',required=True))
    review(required,409)
    assert review(upgrade)['revision']==2
    sql("UPDATE app_packages SET revision=9223372036854775807 WHERE tenant='shop_a' AND id='app_notes'")
    install(approved(upgrade),409)
    sql("UPDATE app_packages SET revision=2 WHERE tenant='shop_a' AND id='app_notes'")
    # Late attachment/event failures roll back package, models, ledger and accounting as one transaction.
    for target,constraint,statement in [
        ('app_fail_model','reject_model',"ALTER TABLE app_models ADD CONSTRAINT reject_model CHECK(app<>'app_fail_model')"),
        ('app_fail_event','reject_install',"ALTER TABLE outbox ADD CONSTRAINT reject_install CHECK(kind<>'app.installed' OR data->>'app'<>'app_fail_event')")]:
        broken=dict(manifest,id=target);prepared=approved(broken);sql(statement);install(prepared,503);sql(f'ALTER TABLE {"app_models" if target=="app_fail_model" else "outbox"} DROP CONSTRAINT {constraint}')
        assert sql(f"SELECT count(*) FROM app_packages WHERE id='{target}'")=='0'
        assert sql(f"SELECT count(*) FROM app_models WHERE app='{target}'")=='0'
        assert sql(f"SELECT count(*) FROM app_versions WHERE app='{target}'")=='0'
        assert sql(f"SELECT count(*) FROM app_storage_usage WHERE app='{target}'")=='0'
    # A failure after the package version and event were inserted still rolls everything back.
    sql("CREATE FUNCTION fixture_bad_model() RETURNS trigger LANGUAGE plpgsql AS $fn$ BEGIN IF NEW.app='app_fail_finish' THEN NEW.definition=jsonb_set(NEW.definition,'{fields}','[]'); END IF; RETURN NEW; END $fn$; CREATE TRIGGER fixture_bad_model BEFORE INSERT ON app_models FOR EACH ROW EXECUTE FUNCTION fixture_bad_model()")
    install(approved(dict(manifest,id='app_fail_finish')),503)
    assert sql("SELECT count(*) FROM app_packages WHERE id='app_fail_finish'")=='0'
    assert sql("SELECT count(*) FROM outbox WHERE kind='app.installed' AND data->>'app'='app_fail_finish'")=='0'
    sql('DROP TRIGGER fixture_bad_model ON app_models; DROP FUNCTION fixture_bad_model()')
    # Actual role cannot mutate immutable history, package identity, quota counters, reserved packages or schemas.
    ticket=g['ticket']('shop_a');prefix=f"BEGIN; SELECT set_config('veyndo.scope_ticket','{ticket}',true); "
    for command in ["UPDATE app_versions SET digest='forged' WHERE tenant='shop_a'", "UPDATE app_packages SET id='other' WHERE tenant='shop_a'", "UPDATE app_packages SET digest='forged' WHERE tenant='shop_a' AND id='advice'", "UPDATE app_storage_usage SET rows=0 WHERE tenant='shop_a'", 'CREATE TABLE public.forbidden(id text)']:
        sql(prefix+command+'; ROLLBACK;',g['READ_URL'],ok=False)
    # Shared record/history/event writes still roll back at the original per-App row budget.
    sql("UPDATE app_storage_usage SET rows=100000 WHERE tenant='shop_a' AND app='app_notes'")
    digest=review(upgrade)['digest']
    call('/v1/records/write',dict(app='app_notes',action='save_note',packageDigest=digest,approve=True,input=dict(id='over_quota',revision=0,fields=dict(title='Quota',enabled=False))),503)
    assert len(read()['elements'])==1
    sql("UPDATE app_storage_usage SET rows=1 WHERE tenant='shop_a' AND app='app_notes'")
    prior_bytes=sql("SELECT bytes FROM app_storage_usage WHERE tenant='shop_a' AND app='app_notes'")
    sql("UPDATE app_storage_usage SET bytes=67108864 WHERE tenant='shop_a' AND app='app_notes'")
    call('/v1/records/write',dict(app='app_notes',action='save_note',packageDigest=digest,approve=True,input=dict(id='one',revision=1,fields=dict(title='Larger than the previous title',enabled=False))),503)
    assert read()['elements'][0]['revision']==1
    sql(f"UPDATE app_storage_usage SET bytes={prior_bytes} WHERE tenant='shop_a' AND app='app_notes'")
    assert rpc('tools/call',dict(name='apps.packages.review',arguments={'manifest':upgrade}))['result']['structuredContent']['revision']==2
    subprocess.run([g['os'].environ.get('NODE','node'),str(root/'data-service/examples/install.mjs')],env=dict(g['os'].environ,APP_DATA_URL=g['BASE'],APP_DATA_TOKEN=g['TOKEN_A'],APP_DATA_SDK_MODULE=g['os'].environ['APP_DATA_SDK_MODULE']),check=True)
    stop();g['running']=g['start'](env);assert review(upgrade)['revision']==2 and read()['elements'][0]['title']=='First'
    print('PASS independent installation: full compiler, exact permission/digest/revision approval, atomic models/versions/budget/events, two shops, live personal authority, immutable upgrades, rollback, SDK/MCP, restart and preserved record/settings flows')
