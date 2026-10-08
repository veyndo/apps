//! Multi-relations keep stable array wire values, composite tenant FKs and shared accounting; staging may clone cyclic graphs atomically.
use super::*;
pub(super) fn suffix(e: &Entity, f: &Field) -> String {
    format!("rel_{}", &hash(&format!("{}:{}", e.name, f.name))[..20])
}
pub(super) fn names(t: &str, m: &Manifest) -> Vec<(String, String)> {
    m.entities
        .iter()
        .flat_map(|e| {
            e.fields
                .iter()
                .filter(|f| f.kind == "relations")
                .map(move |f| {
                    let suffix = suffix(e, f);
                    (table(t, &m.id, &suffix), suffix)
                })
        })
        .collect()
}
pub(super) async fn install(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
) -> Result<()> {
    for e in &m.entities {
        for f in e.fields.iter().filter(|f| f.kind == "relations") {
            let source = table(t, &m.id, &e.name);
            let target = table(t, &m.id, f.references.as_deref().unwrap());
            let links = table(t, &m.id, &suffix(e, f));
            let statement = format!(
                "CREATE TABLE IF NOT EXISTS public.{links}(tenant text NOT NULL, source_id text NOT NULL, target_id text NOT NULL, position integer NOT NULL CHECK(position BETWEEN 1 AND 100), PRIMARY KEY(tenant,source_id,target_id), FOREIGN KEY(tenant,source_id) REFERENCES public.{source}(tenant,id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED, FOREIGN KEY(tenant,target_id) REFERENCES public.{target}(tenant,id) DEFERRABLE INITIALLY DEFERRED); ALTER TABLE public.{links} ENABLE ROW LEVEL SECURITY; ALTER TABLE public.{links} FORCE ROW LEVEL SECURITY"
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
                .execute(&mut **tx)
                .await?;
            let policy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_policies WHERE schemaname='public' AND tablename=$1 AND policyname='tenant_scope')").bind(&links).fetch_one(&mut **tx).await?;
            if !policy {
                let statement = format!(
                    "CREATE POLICY tenant_scope ON public.{links} USING(tenant=current_setting('rac.tenant',true)) WITH CHECK(tenant=current_setting('rac.tenant',true))"
                );
                sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
                    .execute(&mut **tx)
                    .await?;
            }
            let trigger = format!("app_rel_{}", &hash(&f.name)[..16]);
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass($1) AND tgname=$2)").bind(format!("public.{source}")).bind(&trigger).fetch_one(&mut **tx).await?;
            if !exists {
                let statement = format!(
                    "CREATE TRIGGER {trigger} AFTER INSERT OR UPDATE OF {} ON public.{source} FOR EACH ROW EXECUTE FUNCTION app_sync_relations('{links}','{}','{source}')",
                    column(&f.name),
                    f.name
                );
                sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    Ok(())
}
pub(crate) async fn drop_preview(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
) -> Result<()> {
    for (name, _) in names(t, m) {
        let statement = format!("DROP TABLE IF EXISTS public.{name} CASCADE");
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
pub(super) fn valid(value: &Value) -> bool {
    value.as_array().is_some_and(|ids| {
        ids.len() <= 100
            && ids
                .iter()
                .all(|id| id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 100))
            && ids.iter().collect::<std::collections::HashSet<_>>().len() == ids.len()
    })
}
