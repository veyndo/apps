//! Attach the shared PostgreSQL accounting trigger to tenant-specific app tables before any write.
use super::*;
pub(super) async fn attach(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
) -> Result<()> {
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO app_storage_usage(tenant,app) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(t)
        .bind(&m.id)
        .execute(&mut **tx)
        .await?;
    // Installation holds DDL locks on every entity until commit. Recount after a schema
    // change: even a newly added nullable column changes the serialized row size.
    let mut rows = 0i64;
    let mut bytes = 0i64;
    let tables = m
        .entities
        .iter()
        .map(|e| (table(t, &m.id, &e.name), e.name.clone()))
        .chain(relations::names(t, m));
    for (name, suffix) in tables {
        let statement = format!(
            "SELECT count(*) AS rows,coalesce(sum(octet_length(to_jsonb(r)::text)),0)::bigint AS bytes FROM public.{name} r WHERE tenant=$1"
        );
        let count = sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
            .bind(t)
            .fetch_one(&mut **tx)
            .await?;
        rows += count.get::<i64, _>("rows");
        bytes += count.get::<i64, _>("bytes");
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass($1) AND tgname='app_storage_quota')").bind(format!("public.{name}")).fetch_one(&mut **tx).await?;
        if !exists {
            let ddl = format!(
                "CREATE TRIGGER app_storage_quota AFTER INSERT OR UPDATE OR DELETE ON public.{name} FOR EACH ROW EXECUTE FUNCTION app_storage_accounting('{}','{}')",
                m.id, suffix
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
                .execute(&mut **tx)
                .await?;
        }
    }
    sqlx::query(
        "UPDATE app_storage_usage SET rows=$3,bytes=$4,updated_at=now() WHERE tenant=$1 AND app=$2",
    )
    .bind(t)
    .bind(&m.id)
    .bind(rows)
    .bind(bytes)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
