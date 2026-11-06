"""Current administrator settings on the actual Rust HTTP/MCP/SDK process and restricted PostgreSQL login."""
import copy
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor

def run(g):
    sql,literal,request,rpc=(g[n] for n in ('sql','literal','request','rpc'))
    env,root,role=(g[n] for n in ('env','ROOT','ROLE'))
    def stop():
        if g['running']:
            process,log=g['running'];process.terminate();process.wait();log.close();g['running']=None
    assert 'apps.settings.read' not in [t['name'] for t in rpc('tools/list')['result']['tools']]
    assert request('/v1/settings/read',{'app':'advice'})[0]==403
    stop()
    migrated=subprocess.run([g['PSQL'],g['DB_ADMIN'],'-X','-v','ON_ERROR_STOP=1','-v','reader_role='+role,'-f',str(root/'data-service/sql/enable-settings.sql')],capture_output=True,text=True)
    assert migrated.returncode==0,migrated.stderr
    manifest=copy.deepcopy(g['MANIFEST'])
    manifest['settings']=[dict(key='key',label={'en':'Key','de':'Schlüssel'},kind='secret',required=True),dict(key='enabled',label={'en':'Enabled'},kind='boolean'),dict(key='limit',label={'en':'Limit'},kind='integer'),dict(key='label',label={'en':'Label','de':'Bezeichnung'},kind='string')]
    for tenant in ('shop_a','shop_b'):
        sql(f"UPDATE app_packages SET manifest={literal(manifest)} WHERE tenant='{tenant}' AND id='advice'")
    env=dict(env,APP_DATA_SETTINGS='enabled',APP_DATA_SETTINGS_KEY='7a'*32)
    # Expanded roles/schema never silently enable the optional settings profile.
    g['start'](dict(env,APP_DATA_SETTINGS='disabled'),success=False)
    sql(f'GRANT DELETE ON app_service_secrets TO {role}')
    g['start'](env,success=False)
    sql(f'REVOKE DELETE ON app_service_secrets FROM {role}')
    g['running']=g['start'](env)
    claims=g['CLAIMS'][g['TOKEN_A']]
    claims.update(scope='apps.read apps.manage team.manage catalog.read',personal_session=True,live=True)
    g['CLAIMS'][g['TOKEN_B']].update(scope=claims['scope'],personal_session=True,live=True)
    def read(token=None,expected=200):
        status,value,_=request('/v1/settings/read',{'app':'advice'},token=token or g['TOKEN_A']);assert status==expected,(status,value);return value
    first=read();assert first['revision']==0 and first['current'] and first['values']=={}
    assert first['fields'][0]['label']['de']=='Schlüssel'
    digest=first['digest']
    # The separate trusted installation owner, never an API caller, registers the canonical digest.
    def change(revision=0,values=None):
        return dict(app='advice',digest=digest,revision=revision,approve=True,values=values if values is not None else dict(key='synthetic-private-config',enabled=False,limit=0,label=''))
    def save(body,expected=200,token=None):
        status,value,_=request('/v1/settings/save',body,token=token or g['TOKEN_A']);assert status==expected,(status,value);return value
    save(change(),409)
    sql(f"UPDATE app_packages SET digest='{digest}' WHERE id='advice'")
    original=claims.copy()
    for alteration in [dict(personal_session=False),dict(live=False),dict(scope='apps.read apps.manage'),dict(scope='apps.read team.manage'),dict(merchant=False),dict(sub='bootstrap')]:
        claims.update(alteration);read(expected=403);save(change(),403);claims.clear();claims.update(original)
    # Missing claims also deny; scopes alone cannot turn a machine/agent into a personal administrator.
    del claims['personal_session'];read(expected=403);claims.update(original)
    assert request('/v1/settings/read',{'app':'advice','tenant':'shop_b'})[0]==400
    save(dict(change(),approve=False),400)
    save(dict(change(),digest='0'*64),409)
    save(change(values={'undeclared':'private'}),400)
    save(change(values={'enabled':'false'}),400)
    save(change(values={'enabled':True}),400)
    assert save(change())['revision']==1
    visible=read();assert visible['values']==dict(enabled=False,limit=0,label='') and 'key' not in visible['values'] and 'key' in visible['configured']
    assert 'synthetic-private-config' not in json.dumps(visible)
    cipher=sql("SELECT ciphertext FROM app_service_secrets WHERE tenant='shop_a'")
    assert 'synthetic-private-config' not in cipher
    assert 'synthetic-private-config' not in sql("SELECT data::text FROM outbox WHERE kind='app.secret_rotated'")
    assert read(token=g['TOKEN_B'])['revision']==0
    # Same key in another shop is an independent configuration.
    assert save(change(values={'key':'other-shop-secret'}),token=g['TOKEN_B'])['revision']==1
    assert read()['revision']==1
    assert save(change(1,{'label':'Revised'}))['revision']==2
    assert 'key' in read()['configured']
    save(change(2,{'key':None}),400)
    competing=change(2,{'label':'Race'})
    with ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(lambda _:request('/v1/settings/save',competing)[0],range(2)))==[200,409]
    assert read()['revision']==3
    sql("ALTER TABLE outbox ADD CONSTRAINT reject_settings CHECK(kind<>'app.secret_rotated') NOT VALID")
    save(change(3,{'label':'Must roll back'}),503)
    assert read()['revision']==3 and read()['values']['label']=='Race'
    sql('ALTER TABLE outbox DROP CONSTRAINT reject_settings')
    # A changed package never inherits an earlier version's secrets.
    newer=copy.deepcopy(manifest);newer['version']='0.2.0'
    sql(f"UPDATE app_packages SET manifest={literal(newer)} WHERE tenant='shop_a' AND id='advice'")
    upgrade=read();assert not upgrade['current'] and upgrade['values']=={} and upgrade['configured']==[]
    save(change(3),409)
    digest=upgrade['digest']
    sql(f"UPDATE app_packages SET digest='{digest}' WHERE tenant='shop_a' AND id='advice'")
    save(change(3,{'label':'No inherited secret'}),400)
    assert save(change(3))['revision']==4
    # Package definition writes remain denied on the actual serving login.
    ticket=g['ticket']('shop_a')
    sql(f"BEGIN; SELECT set_config('veyndo.scope_ticket','{ticket}',true); UPDATE app_packages SET digest='bad' WHERE tenant='shop_a'; ROLLBACK;",g['READ_URL'],ok=False)
    sql(f"BEGIN; SELECT set_config('veyndo.scope_ticket','{ticket}',true); DELETE FROM app_service_secrets WHERE tenant='shop_a'; ROLLBACK;",g['READ_URL'],ok=False)
    sql("UPDATE app_packages SET active=false WHERE tenant='shop_a' AND id='advice'");read(expected=409)
    sql("UPDATE app_packages SET active=true WHERE tenant='shop_a' AND id='advice'")
    assert rpc('tools/call',dict(name='apps.settings.read',arguments={'app':'advice'}))['result']['structuredContent']==read()
    assert rpc('tools/call',dict(name='apps.settings.save',arguments=dict(change(4),approve=False)))['result']['isError']
    subprocess.run([g['os'].environ.get('NODE','node'),str(root/'data-service/examples/settings.mjs')],env=dict(g['os'].environ,APP_DATA_URL=g['BASE'],APP_DATA_TOKEN=g['TOKEN_A'],APP_DATA_SDK_MODULE=g['os'].environ['APP_DATA_SDK_MODULE']),check=True)
    stop();g['running']=g['start'](dict(env,APP_DATA_SETTINGS_KEY='8b'*32));read(expected=503)
    save(change(5,{'label':'Wrong key'}),503)
    stop();g['running']=g['start'](env);assert read()['revision']==5
    # Ciphertext cannot be moved between tenant identities; associated data binds all coordinates.
    own=sql("SELECT ciphertext FROM app_service_secrets WHERE tenant='shop_a'")
    other=sql("SELECT ciphertext FROM app_service_secrets WHERE tenant='shop_b'")
    sql(f"UPDATE app_service_secrets SET ciphertext='{other}' WHERE tenant='shop_a'");read(expected=503)
    sql(f"UPDATE app_service_secrets SET ciphertext='{own}' WHERE tenant='shop_a'")
    assert read()['revision']==5
    claims['scope']='apps.read';read(expected=403)
    print('PASS independent settings: personal live authority, encrypted partial updates, secret-safe HTTP/MCP/SDK, two shops, package/revision CAS, rollback, key/reconnect and no package/schema authority')
