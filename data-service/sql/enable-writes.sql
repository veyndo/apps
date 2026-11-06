-- Explicit optional upgrade of a dedicated reader database; run only as its original migration owner.
-- Serving code never executes this file. reader_role must be the configured existing restricted login.
\set ON_ERROR_STOP on
BEGIN;
SELECT set_config('app.install_reader', :'reader_role', true);
DO $fn$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM public.app_host_schema WHERE id AND version=1 AND component='veyndo-app-data-reader')
 OR NOT EXISTS(SELECT 1 FROM veyndo_authority.configuration WHERE migration_role=session_user AND enforced AND tenant_login=current_setting('app.install_reader')) THEN
  RAISE EXCEPTION 'Managed writes require schema 1, the original migration owner and configured reader login';
 END IF;
END $fn$;
CREATE TABLE app_record_versions (
 tenant text NOT NULL,app text NOT NULL,entity text NOT NULL,record_id text NOT NULL,
 revision bigint NOT NULL CHECK(revision>0),fields jsonb NOT NULL,read_scopes jsonb NOT NULL CHECK(jsonb_typeof(read_scopes)='array'),
 actor text,source text NOT NULL,created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant,app,entity,record_id,revision),
 FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id)
);
CREATE TABLE outbox (
 id bigserial PRIMARY KEY,tenant text NOT NULL,kind text NOT NULL CHECK(kind='app.record.changed'),
 data jsonb NOT NULL CHECK(jsonb_typeof(data)='object' AND data-ARRAY['app','entity','id','revision']='{}'::jsonb
  AND jsonb_typeof(data->'app')='string' AND jsonb_typeof(data->'entity')='string'
  AND jsonb_typeof(data->'id')='string' AND jsonb_typeof(data->'revision')='number'),
 created_at timestamptz NOT NULL DEFAULT now()
);
DO $fn$ DECLARE t text; r name:=current_setting('app.install_reader'); BEGIN
 FOREACH t IN ARRAY ARRAY['app_record_versions','outbox'] LOOP
  EXECUTE format('ALTER TABLE public.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE public.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY tenant_scope ON public.%I USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>''migration'')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>''tenant''))',t);
 END LOOP;
 -- SELECT FOR SHARE needs column UPDATE privilege; its row policy admits a lock
 -- while WITH CHECK false rejects every actual package edit by the serving login.
 DROP POLICY tenant_scope ON public.app_packages;
 CREATE POLICY tenant_read ON public.app_packages FOR SELECT USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
 CREATE POLICY writer_package_lock ON public.app_packages FOR UPDATE USING(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant')) WITH CHECK(false);
 CREATE POLICY migration_package ON public.app_packages FOR ALL USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false)) WITH CHECK(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false));
 EXECUTE format('GRANT UPDATE(active) ON public.app_packages TO %I',r);
 EXECUTE format('GRANT INSERT,UPDATE(fields,revision) ON public.app_records TO %I',r);
 EXECUTE format('GRANT SELECT,INSERT,DELETE ON public.app_record_versions TO %I',r);
 EXECUTE format('GRANT INSERT ON public.outbox TO %I',r);
 EXECUTE format('GRANT USAGE ON SEQUENCE public.outbox_id_seq TO %I',r);
END $fn$;
UPDATE public.app_host_schema SET version=2 WHERE id;
COMMIT;
