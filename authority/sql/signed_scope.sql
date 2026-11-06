-- Native 099 authenticated scope prefix, source fd370580d97b7133272ba96fb9d31e70a357d7f7.
-- Expand first: existing replicas keep their policies until the signed-pool cutover is verified.
CREATE EXTENSION IF NOT EXISTS pgcrypto;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
CREATE SCHEMA veyndo_authority;
REVOKE ALL ON SCHEMA veyndo_authority FROM PUBLIC;
CREATE TABLE veyndo_authority.configuration (
 id boolean PRIMARY KEY DEFAULT true CHECK(id), enforced boolean NOT NULL DEFAULT false,
 system_role name, migration_role name NOT NULL, tenant_login name
);
CREATE TABLE veyndo_authority.keys (
 id text PRIMARY KEY, secret bytea NOT NULL CHECK(octet_length(secret)=32),
 active boolean NOT NULL, verify_until timestamptz,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX scope_active_key ON veyndo_authority.keys(active) WHERE active;
DO $fn$
DECLARE crypto_schema name;
BEGIN
 INSERT INTO veyndo_authority.configuration(migration_role) VALUES(session_user);
 SELECT n.nspname INTO STRICT crypto_schema FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='pgcrypto';
 EXECUTE format('CREATE FUNCTION veyndo_authority.mac(payload bytea,secret bytea) RETURNS bytea LANGUAGE sql IMMUTABLE STRICT SET search_path=pg_catalog,pg_temp AS %L',
  format('SELECT %I.hmac(payload,secret,''sha256'')',crypto_schema));
 EXECUTE format('CREATE FUNCTION veyndo_authority.random_key() RETURNS bytea LANGUAGE sql VOLATILE SET search_path=pg_catalog,pg_temp AS %L',
  format('SELECT %I.gen_random_bytes(32)',crypto_schema));
 INSERT INTO veyndo_authority.keys(id,secret,active) VALUES(replace(gen_random_uuid()::text,'-',''),veyndo_authority.random_key(),true);
END;
$fn$;

-- session_user is the actual database login, not this SECURITY DEFINER function's owner.
CREATE FUNCTION public.veyndo_scope_key() RETURNS TABLE(id text,secret text,system_role name,tenant_login name)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $fn$
DECLARE c veyndo_authority.configuration;
BEGIN
 SELECT * INTO STRICT c FROM veyndo_authority.configuration;
 IF session_user IS DISTINCT FROM c.system_role AND session_user IS DISTINCT FROM c.migration_role THEN
  RAISE EXCEPTION 'Database signing material requires the system connection' USING ERRCODE='42501';
 END IF;
 RETURN QUERY SELECT k.id,encode(k.secret,'hex'),c.system_role,c.tenant_login FROM veyndo_authority.keys k WHERE k.active;
END;
$fn$;

CREATE FUNCTION public.veyndo_authenticated_scope() RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $fn$
DECLARE c veyndo_authority.configuration; parts text[]; key_bytes bytea;
 expected bytea; supplied bytea; deadline bigint; epoch_now bigint; body text;
BEGIN
 SELECT * INTO STRICT c FROM veyndo_authority.configuration;
 IF session_user IS NOT DISTINCT FROM c.migration_role THEN
  RETURN jsonb_build_object('system',true,'migration',true,'authenticated',true);
 END IF;
 body:=current_setting('veyndo.scope_ticket',true);
 IF body IS NULL OR length(body)>512 THEN RETURN '{}'::jsonb; END IF;
 parts:=string_to_array(body,':');
 IF cardinality(parts)<>8 OR parts[1]<>'v1' OR parts[2]!~'^[a-f0-9]{32}$'
  OR parts[3]!~'^[0-9]{1,12}$' OR parts[4] NOT IN ('S','T','P','A','D')
  OR parts[5]!~'^[A-Za-z0-9_-]{0,64}$' OR parts[6]!~'^[A-Za-z0-9_-]{0,64}$'
  OR parts[7]!~'^[a-f0-9]{32}$' OR parts[8]!~'^[a-f0-9]{64}$' THEN RETURN '{}'::jsonb; END IF;
 deadline:=parts[3]::bigint; epoch_now:=floor(extract(epoch FROM statement_timestamp()))::bigint;
 IF deadline<epoch_now OR deadline>epoch_now+600 THEN RETURN '{}'::jsonb; END IF;
 SELECT k.secret INTO key_bytes FROM veyndo_authority.keys k
 WHERE k.id=parts[2] AND (k.active OR k.verify_until>statement_timestamp());
 IF key_bytes IS NULL THEN RETURN '{}'::jsonb; END IF;
 body:=array_to_string(parts[1:7],':')||':'||session_user;
 expected:=veyndo_authority.mac(convert_to(body,'UTF8'),key_bytes);
 supplied:=decode(parts[8],'hex');
 -- Blinded comparison does not expose the position of an incorrect MAC byte.
 IF veyndo_authority.mac(expected,key_bytes)<>veyndo_authority.mac(supplied,key_bytes) THEN RETURN '{}'::jsonb; END IF;
 IF parts[4]='S' THEN
  IF session_user IS DISTINCT FROM c.system_role OR parts[5]<>'' OR parts[6]<>'' THEN RETURN '{}'::jsonb; END IF;
  RETURN jsonb_build_object('system',true,'authenticated',true);
 END IF;
 IF session_user<>c.tenant_login OR session_user IS NOT DISTINCT FROM c.system_role THEN RETURN '{}'::jsonb; END IF;
 IF parts[4]='D' THEN RETURN jsonb_build_object('system',false,'authenticated',true); END IF;
 IF parts[5]='' OR parts[4] IN ('A','P') AND parts[6]='' OR parts[4]='T' AND parts[6]<>'' THEN RETURN '{}'::jsonb; END IF;
 RETURN jsonb_build_object('system',false,'authenticated',true,'tenant',parts[5],
  'actor',nullif(parts[6],''),'personalActor',CASE WHEN parts[4]='P' THEN parts[6] ELSE NULL END);
END;
$fn$;

CREATE FUNCTION public.veyndo_scope_guard() RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $fn$
BEGIN
 IF NOT (SELECT enforced FROM veyndo_authority.configuration) THEN RETURN '{"system":true}'::jsonb; END IF;
 RETURN public.veyndo_authenticated_scope();
END;
$fn$;
