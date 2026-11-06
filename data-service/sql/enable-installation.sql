-- Explicit schema-3 upgrade by its original migration owner. No serving owner/DDL credentials.
\set ON_ERROR_STOP on
BEGIN;
SELECT set_config('app.install_reader', :'reader_role', true);
DO $fn$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM app_host_schema WHERE id AND version=3 AND component='veyndo-app-data-reader')
 OR NOT EXISTS(SELECT 1 FROM veyndo_authority.configuration WHERE migration_role=session_user AND enforced AND tenant_login=current_setting('app.install_reader')) THEN
  RAISE EXCEPTION 'Installation requires schema 3, its original migration owner and configured login';
 END IF;
END $fn$;
ALTER TABLE app_packages ADD COLUMN version text NOT NULL DEFAULT '';
ALTER TABLE app_packages ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK(revision>0);
UPDATE app_packages SET version=manifest->>'version';
ALTER TABLE app_packages ALTER COLUMN active SET DEFAULT true;
CREATE TABLE app_versions (
 tenant text NOT NULL,app text NOT NULL,version text NOT NULL,digest text NOT NULL,manifest jsonb NOT NULL,
 PRIMARY KEY(tenant,app,version),FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id)
);
-- Preserve prior trusted digest registrations in the immutable ledger; unregistered rows are not adopted.
INSERT INTO app_versions SELECT tenant,id,version,digest,manifest FROM app_packages WHERE digest~'^[a-f0-9]{64}$';
CREATE TABLE app_storage_usage (
 tenant text NOT NULL,app text NOT NULL,rows bigint NOT NULL DEFAULT 0,bytes bigint NOT NULL DEFAULT 0,updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant,app),FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id),
 CHECK(rows>=0 AND rows<=100000),CHECK(bytes>=0 AND bytes<=67108864)
);
INSERT INTO app_storage_usage(tenant,app,rows,bytes)
 SELECT p.tenant,p.id,count(r.id),coalesce(sum(octet_length((r.fields||jsonb_build_object('tenant',r.tenant,'id',r.id,'revision',r.revision))::text)),0)
 FROM app_packages p LEFT JOIN app_records r ON r.tenant=p.tenant AND r.app=p.id GROUP BY p.tenant,p.id;
ALTER TABLE app_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE app_storage_usage ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_storage_usage FORCE ROW LEVEL SECURITY;
CREATE POLICY version_read ON app_versions FOR SELECT USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
CREATE POLICY version_insert ON app_versions FOR INSERT WITH CHECK(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR (tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND left(app,4)='app_'));
CREATE POLICY tenant_scope ON app_storage_usage USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
CREATE POLICY installer_insert ON app_packages FOR INSERT WITH CHECK(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND left(id,4)='app_');
CREATE POLICY installer_update ON app_packages FOR UPDATE USING(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND left(id,4)='app_') WITH CHECK(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND left(id,4)='app_');
DROP POLICY tenant_scope ON app_models;
CREATE POLICY model_read ON app_models FOR SELECT USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
CREATE POLICY installer_models ON app_models FOR ALL USING(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND left(app,4)='app_') WITH CHECK(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND left(app,4)='app_');
CREATE POLICY migration_models ON app_models FOR ALL USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false)) WITH CHECK(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false));
-- Portable records have no relationships: one original native budget row and original wire-byte formula per record.
CREATE FUNCTION app_data_accounting() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public,pg_temp AS $fn$
DECLARE shop text; package text; old_bytes bigint:=0; next_bytes bigint:=0; delta_rows bigint;
BEGIN
 IF TG_OP='UPDATE' AND (NEW.tenant,NEW.app,NEW.entity,NEW.id) IS DISTINCT FROM (OLD.tenant,OLD.app,OLD.entity,OLD.id) THEN RAISE EXCEPTION 'App identity cannot change' USING ERRCODE='42501'; END IF;
 shop:=CASE WHEN TG_OP='DELETE' THEN OLD.tenant ELSE NEW.tenant END;
 package:=CASE WHEN TG_OP='DELETE' THEN OLD.app ELSE NEW.app END;
 IF TG_OP<>'INSERT' THEN old_bytes:=octet_length((OLD.fields||jsonb_build_object('tenant',OLD.tenant,'id',OLD.id,'revision',OLD.revision))::text); END IF;
 IF TG_OP<>'DELETE' THEN next_bytes:=octet_length((NEW.fields||jsonb_build_object('tenant',NEW.tenant,'id',NEW.id,'revision',NEW.revision))::text); END IF;
 delta_rows:=(CASE WHEN TG_OP='DELETE' THEN 0 ELSE 1 END)-(CASE WHEN TG_OP='INSERT' THEN 0 ELSE 1 END);
 UPDATE app_storage_usage SET rows=rows+delta_rows,bytes=bytes+next_bytes-old_bytes,updated_at=now() WHERE tenant=shop AND app=package;
 IF NOT FOUND THEN RAISE EXCEPTION 'App storage quota is not initialized' USING ERRCODE='42501'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 RETURN NEW;
END $fn$;
REVOKE ALL ON FUNCTION app_data_accounting() FROM PUBLIC;
CREATE FUNCTION app_data_record_version() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $fn$
BEGIN
 SELECT version INTO STRICT NEW.version FROM app_models WHERE tenant=NEW.tenant AND app=NEW.app AND entity=NEW.entity;
 RETURN NEW;
END $fn$;
REVOKE ALL ON FUNCTION app_data_record_version() FROM PUBLIC;
CREATE TRIGGER app_data_version BEFORE INSERT OR UPDATE ON app_records FOR EACH ROW EXECUTE FUNCTION app_data_record_version();
CREATE TRIGGER app_data_budget AFTER INSERT OR UPDATE OR DELETE ON app_records FOR EACH ROW EXECUTE FUNCTION app_data_accounting();
ALTER TABLE outbox DROP CONSTRAINT outbox_payload;
ALTER TABLE outbox ADD CONSTRAINT outbox_payload CHECK ((
 (kind='app.record.changed' AND jsonb_typeof(data)='object' AND data-ARRAY['app','entity','id','revision']='{}'::jsonb AND jsonb_typeof(data->'app')='string' AND jsonb_typeof(data->'entity')='string' AND jsonb_typeof(data->'id')='string' AND jsonb_typeof(data->'revision')='number')
 OR (kind='app.secret_rotated' AND jsonb_typeof(data)='object' AND data-ARRAY['app','kind','revision','actor']='{}'::jsonb AND jsonb_typeof(data->'app')='string' AND data->>'kind'='settings' AND jsonb_typeof(data->'revision')='number' AND jsonb_typeof(data->'actor')='string')
 OR (kind='app.installed' AND jsonb_typeof(data)='object' AND data-ARRAY['app','version','digest']='{}'::jsonb AND jsonb_typeof(data->'app')='string' AND jsonb_typeof(data->'version')='string' AND jsonb_typeof(data->'digest')='string')
) IS TRUE);
DO $fn$ DECLARE r name:=current_setting('app.install_reader'); BEGIN
 EXECUTE format('GRANT INSERT,UPDATE(version,manifest,digest,revision) ON app_packages TO %I',r);
 EXECUTE format('GRANT INSERT,UPDATE(version,definition) ON app_models TO %I',r);
 EXECUTE format('GRANT SELECT,INSERT ON app_versions TO %I',r);
 EXECUTE format('GRANT SELECT,INSERT(tenant,app) ON app_storage_usage TO %I',r);
END $fn$;
UPDATE app_host_schema SET version=4 WHERE id;
COMMIT;
