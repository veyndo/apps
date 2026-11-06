-- Explicit schema-4 supervisor upgrade by its original migration owner; never run by the serving process.
\set ON_ERROR_STOP on
BEGIN;
SELECT set_config('app.install_reader', :'reader_role', true);
DO $fn$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM app_host_schema WHERE id AND version=4 AND component='veyndo-app-data-reader')
 OR NOT EXISTS(SELECT 1 FROM veyndo_authority.configuration WHERE migration_role=session_user AND enforced AND tenant_login=current_setting('app.install_reader')) THEN
  RAISE EXCEPTION 'Job supervision requires schema 4, its original migration owner and configured login';
 END IF;
END $fn$;
CREATE TABLE app_jobs (
 tenant text NOT NULL,app text NOT NULL,id text NOT NULL CHECK(length(id) BETWEEN 1 AND 100),
 action text NOT NULL,actor text NOT NULL,permission text NOT NULL,
 package_digest text NOT NULL,request_key text NOT NULL,input_digest text NOT NULL,
 input jsonb NOT NULL,result jsonb,status text NOT NULL DEFAULT 'queued',
 progress integer NOT NULL DEFAULT 0 CHECK(progress BETWEEN 0 AND 100),
 message jsonb NOT NULL DEFAULT '{}',lease text,lease_until timestamptz,started_at timestamptz,
 created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 PRIMARY KEY(tenant,app,id),UNIQUE(tenant,app,request_key),
 FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id),
 CHECK(status IN ('queued','running','succeeded','failed','uncertain','cancel_requested','cancelled')),
 CHECK(octet_length(input::text)<=20000),CHECK(result IS NULL OR octet_length(result::text)<=40000)
);
CREATE INDEX app_jobs_active ON app_jobs(tenant,app,created_at DESC);
ALTER TABLE app_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_read ON app_jobs FOR SELECT USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
CREATE POLICY supervisor_update ON app_jobs FOR UPDATE USING(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant')) WITH CHECK(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
CREATE POLICY supervisor_archive ON app_jobs FOR DELETE USING(tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant') AND status IN ('succeeded','failed','cancelled'));
CREATE POLICY migration_jobs ON app_jobs FOR ALL USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false)) WITH CHECK(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false));
-- Keep the complete prior minimized payload predicate; add only the original job signal/control metadata shapes.
DO $fn$ DECLARE previous text; BEGIN
 SELECT pg_get_expr(conbin,conrelid) INTO STRICT previous FROM pg_constraint WHERE conrelid='outbox'::regclass AND conname='outbox_payload';
 ALTER TABLE outbox DROP CONSTRAINT outbox_payload;
 EXECUTE format($sql$ALTER TABLE outbox ADD CONSTRAINT outbox_payload CHECK (((%s)
 OR (kind~'^app\.[A-Za-z0-9_]{1,64}\.job_[A-Za-z0-9_]{1,64}$' AND jsonb_typeof(data)='object' AND data-'jobId'='{}'::jsonb AND jsonb_typeof(data->'jobId')='string')
 OR (kind='app.job_controlled' AND jsonb_typeof(data)='object' AND data-ARRAY['app','jobId','operation','actor']='{}'::jsonb AND jsonb_typeof(data->'app')='string' AND jsonb_typeof(data->'jobId')='string' AND jsonb_typeof(data->'actor')='string' AND data->>'operation' IN ('cancel','retry','resolve_cancelled','archive'))
 ) IS TRUE)$sql$,previous);
 EXECUTE format('GRANT SELECT,DELETE,UPDATE(status,lease,lease_until,actor,package_digest,progress,started_at,revision,updated_at) ON app_jobs TO %I',current_setting('app.install_reader'));
END $fn$;
UPDATE app_host_schema SET version=5 WHERE id;
COMMIT;
