-- Explicit optional schema-2 upgrade by the original migration owner. Serving code never executes DDL.
\set ON_ERROR_STOP on
BEGIN;
SELECT set_config('app.install_reader', :'reader_role', true);
DO $fn$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM public.app_host_schema WHERE id AND version=2 AND component='veyndo-app-data-reader')
 OR NOT EXISTS(SELECT 1 FROM veyndo_authority.configuration WHERE migration_role=session_user AND enforced AND tenant_login=current_setting('app.install_reader')) THEN
  RAISE EXCEPTION 'Settings require schema 2, the original migration owner and configured writer login';
 END IF;
END $fn$;
-- Same immutable digest registration used by the native trusted installation owner.
-- Existing packages start unapproved for configuration writes until that owner sets their canonical digest.
ALTER TABLE app_packages ADD COLUMN digest text NOT NULL DEFAULT '';
CREATE TABLE app_service_secrets (
 tenant text NOT NULL,app text NOT NULL,kind text NOT NULL CHECK(kind='settings'),
 ciphertext text NOT NULL CHECK(octet_length(ciphertext)<=24000),package_digest text NOT NULL CHECK(package_digest~'^[a-f0-9]{64}$'),
 revision bigint NOT NULL CHECK(revision>0),actor text NOT NULL,rotated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant,app,kind),FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id)
);
ALTER TABLE app_service_secrets ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_service_secrets FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_scope ON app_service_secrets USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>'migration')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>'tenant'));
ALTER TABLE outbox DROP CONSTRAINT outbox_kind_check;
ALTER TABLE outbox DROP CONSTRAINT outbox_data_check;
ALTER TABLE outbox ADD CONSTRAINT outbox_payload CHECK (
 (kind='app.record.changed' AND jsonb_typeof(data)='object' AND data-ARRAY['app','entity','id','revision']='{}'::jsonb
  AND jsonb_typeof(data->'app')='string' AND jsonb_typeof(data->'entity')='string' AND jsonb_typeof(data->'id')='string' AND jsonb_typeof(data->'revision')='number')
 OR (kind='app.secret_rotated' AND jsonb_typeof(data)='object' AND data-ARRAY['app','kind','revision','actor']='{}'::jsonb
  AND jsonb_typeof(data->'app')='string' AND data->>'kind'='settings' AND jsonb_typeof(data->'revision')='number' AND jsonb_typeof(data->'actor')='string')
);
DO $fn$ BEGIN
 EXECUTE format('GRANT SELECT,INSERT,UPDATE(ciphertext,package_digest,revision,actor,rotated_at) ON public.app_service_secrets TO %I',current_setting('app.install_reader'));
END $fn$;
UPDATE app_host_schema SET version=3 WHERE id;
COMMIT;
