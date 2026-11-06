-- Explicit psql migration against a fresh dedicated database. Required: -v reader_role=existing_restricted_login.
-- Never executed by the serving process. Refuse existing schema objects instead of mutating a native Core database.
\set ON_ERROR_STOP on
BEGIN;
\ir ../../authority/sql/signed_scope.sql
CREATE TABLE app_host_schema (id boolean PRIMARY KEY DEFAULT true CHECK(id), component text NOT NULL, version integer NOT NULL);
INSERT INTO app_host_schema(component,version) VALUES('veyndo-app-data-reader',1);
CREATE TABLE app_packages (tenant text NOT NULL,id text NOT NULL,manifest jsonb NOT NULL CHECK(octet_length(manifest::text)<=262144),active boolean NOT NULL DEFAULT false,PRIMARY KEY(tenant,id));
CREATE TABLE app_models (tenant text NOT NULL,app text NOT NULL,entity text NOT NULL,version text NOT NULL,definition jsonb NOT NULL,PRIMARY KEY(tenant,app,entity),FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id));
CREATE TABLE app_records (tenant text NOT NULL,app text NOT NULL,entity text NOT NULL,id text NOT NULL CHECK(length(id) BETWEEN 1 AND 100),revision bigint NOT NULL CHECK(revision>0),version text NOT NULL DEFAULT '',fields jsonb NOT NULL CHECK(jsonb_typeof(fields)='object' AND octet_length(fields::text)<=65536),PRIMARY KEY(tenant,app,entity,id),FOREIGN KEY(tenant,app,entity) REFERENCES app_models(tenant,app,entity));
DO $fn$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['app_packages','app_models','app_records'] LOOP
  EXECUTE format('ALTER TABLE public.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE public.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY tenant_scope ON public.%I USING(coalesce((SELECT (public.veyndo_authenticated_scope()->>''migration'')::boolean),false) OR tenant=(SELECT public.veyndo_authenticated_scope()->>''tenant''))',t);
 END LOOP;
END $fn$;
SELECT set_config('app.install_reader', :'reader_role', true);
DO $fn$ DECLARE r name:=current_setting('app.install_reader'); BEGIN
 IF r=session_user OR NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=r AND rolcanlogin AND NOT(rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb OR rolreplication OR rolinherit))
 OR EXISTS(SELECT 1 FROM pg_roles x WHERE x.rolname<>r AND pg_has_role(r,x.oid,'MEMBER'))
 OR EXISTS(SELECT 1 FROM pg_class WHERE relowner=(SELECT oid FROM pg_roles WHERE rolname=r)) THEN
  RAISE EXCEPTION 'Use an existing restricted NOINHERIT reader login, distinct from the migration owner';
 END IF;
 EXECUTE format('REVOKE CREATE,TEMPORARY ON DATABASE %I FROM PUBLIC',current_database());
 EXECUTE format('REVOKE ALL ON DATABASE %I FROM %I',current_database(),r);
 EXECUTE format('GRANT CONNECT ON DATABASE %I TO %I',current_database(),r);
 EXECUTE format('REVOKE ALL ON SCHEMA public FROM %I',r);
 EXECUTE format('GRANT USAGE ON SCHEMA public TO %I',r);
 EXECUTE format('GRANT SELECT ON public.app_host_schema,public.app_packages,public.app_models,public.app_records TO %I',r);
 UPDATE veyndo_authority.configuration SET tenant_login=r,enforced=true;
END $fn$;
CREATE FUNCTION public.veyndo_scope_status() RETURNS jsonb LANGUAGE sql STABLE SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $fn$
 SELECT jsonb_build_object('enforced',enforced,'tenantRole',tenant_login) FROM veyndo_authority.configuration;
$fn$;
COMMIT;
