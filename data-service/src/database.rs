//! Dedicated read-only database connection and strict startup checks; no migrations, owner credentials or system pool in serving code.
use crate::{
    App,
    error::{Problem, Result},
};
use axum::http::StatusCode;
use sqlx::{PgPool, postgres::PgPoolOptions};
pub fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "data_unavailable",
        "App data could not be accessed; check the current revision before retrying an uncertain write",
    )
}
pub async fn connect(
    url: &str,
    role: &str,
    writes: bool,
    settings: bool,
    installs: bool,
) -> std::result::Result<PgPool, &'static str> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect(url)
        .await
        .map_err(|_| "App Data database connection failed")?;
    let (valid,): (bool,) = sqlx::query_as(r#"SELECT r.rolname=$1 AND NOT (r.rolsuper OR r.rolbypassrls OR r.rolcreaterole OR r.rolcreatedb OR r.rolreplication OR r.rolinherit)
      AND NOT has_database_privilege(current_database(),'CREATE,TEMP')
      AND NOT EXISTS(SELECT 1 FROM pg_roles x WHERE x.oid<>r.oid AND pg_has_role(r.oid,x.oid,'MEMBER'))
      AND NOT EXISTS(SELECT 1 FROM pg_class c WHERE c.relowner=r.oid)
      AND NOT EXISTS(SELECT 1 FROM pg_namespace n WHERE n.nspname NOT LIKE 'pg_%' AND has_schema_privilege(n.oid,'CREATE'))
      AND NOT has_schema_privilege('veyndo_authority','USAGE')
      AND NOT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind='r' AND (has_table_privilege(c.oid,'INSERT,UPDATE,DELETE,TRUNCATE,TRIGGER,REFERENCES') OR has_any_column_privilege(c.oid,'INSERT,UPDATE,REFERENCES')) AND NOT ($2 AND c.relname IN ('app_records','app_record_versions','outbox','app_packages')) AND NOT ($3 AND c.relname='app_service_secrets') AND NOT ($4 AND c.relname IN ('app_models','app_versions','app_storage_usage')))
      AND (SELECT version=CASE WHEN $4 THEN 4 WHEN $3 THEN 3 WHEN $2 THEN 2 ELSE 1 END AND component='veyndo-app-data-reader' FROM public.app_host_schema WHERE id)
      AND (public.veyndo_scope_status()->>'enforced')::boolean
      AND public.veyndo_scope_status()->>'tenantRole'=r.rolname
      AND (SELECT count(*)=3 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relname IN ('app_packages','app_models','app_records') AND c.relrowsecurity AND c.relforcerowsecurity)
      FROM pg_roles r WHERE r.rolname=session_user"#).bind(role).bind(writes).bind(settings).bind(installs).fetch_one(&pool).await.map_err(|_|"Unsupported App Data schema or database role")?;
    if !valid {
        return Err(
            "App Data requires the supported schema and a dedicated restricted login for its selected profile",
        );
    }
    if writes {
        crate::write_role::check(&pool, installs).await?;
    }
    if settings {
        crate::settings_role::check(&pool).await?;
    }
    if installs {
        crate::install_role::check(&pool).await?;
    }
    Ok(pool)
}
impl App {
    pub async fn begin(
        &self,
        p: &crate::identity::Principal,
    ) -> Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        self.begin_scoped(p, false).await
    }
    pub async fn begin_write(
        &self,
        p: &crate::identity::Principal,
    ) -> Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        self.begin_scoped(p, true).await
    }
    async fn begin_scoped(
        &self,
        p: &crate::identity::Principal,
        write: bool,
    ) -> Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let ticket = self
            .config
            .signer
            .actor_ticket(&p.tenant, &p.subject)
            .map_err(|_| unavailable())?;
        let mut tx = self
            .pool
            .begin_with(if write {
                "BEGIN READ WRITE"
            } else {
                "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY"
            })
            .await
            .map_err(|_| unavailable())?;
        sqlx::query("SELECT set_config('veyndo.scope_ticket',$1,true),set_config('statement_timeout','2000',true),set_config('search_path','public,pg_catalog',true)").bind(ticket).execute(&mut *tx).await.map_err(|_|unavailable())?;
        let checked:bool=sqlx::query_scalar("SELECT public.veyndo_authenticated_scope()->>'tenant'=$1 AND public.veyndo_authenticated_scope()->>'actor'=$2").bind(&p.tenant).bind(&p.subject).fetch_one(&mut *tx).await.map_err(|_|unavailable())?;
        if write {
            sqlx::query("SELECT set_config('veyndo.actor',$1,true),set_config('veyndo.source','app.save',true)").bind(&p.subject).execute(&mut *tx).await.map_err(|_|unavailable())?;
        }
        if !checked {
            return Err(unavailable());
        }
        Ok(tx)
    }
}
