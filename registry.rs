//! Atomic installation and additive schema upgrades; immutable version digests preserve history.
use super::*;
pub(crate) async fn package(a: &App, t: &str, id: &str, active: bool) -> Result<Manifest> {
    let row = sqlx::query("SELECT manifest,active FROM app_packages WHERE tenant=$1 AND id=$2")
        .bind(t)
        .bind(id)
        .fetch_optional(&a.db)
        .await?
        .ok_or(Error(StatusCode::NOT_FOUND, "App not installed".into()))?;
    if active && !row.get::<bool, _>("active") {
        return Err(conflict("App is inactive"));
    }
    serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid installed manifest"))
}
pub(crate) async fn install(a: &App, t: &str, m: Manifest) -> Result<Value> {
    let mut tx = a.db.begin().await?;
    let result = install_tx(&mut tx, t, m).await?;
    tx.commit().await?;
    Ok(result)
}
pub(crate) async fn install_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: Manifest,
) -> Result<Value> {
    validate(&m)?;
    if let Some(c) = &m.configuration {
        let entity = m.entities.iter().find(|e| e.name == c.entity).unwrap();
        data::fields(entity, &c.default_fields)?;
        let fee = c.default_fields[&c.price_field]
            .as_i64()
            .ok_or(bad("Default price required"))?;
        runtime::validate_fee(c, fee).await?;
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7))")
        .bind(&m.id)
        .execute(&mut **tx)
        .await?;
    let value = json!(m);
    let digest = hash(&value.to_string());
    if let Some(old) =
        sqlx::query("SELECT digest FROM app_versions WHERE tenant=$1 AND app=$2 AND version=$3")
            .bind(t)
            .bind(&m.id)
            .bind(&m.version)
            .fetch_optional(&mut **tx)
            .await?
        && old.get::<String, _>("digest") != digest
    {
        return Err(conflict("Published app version is immutable"));
    }
    // Versions can coexist across shops; shared physical schemas grow monotonically.
    if let Some(row) = sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2")
        .bind(t)
        .bind(&m.id)
        .fetch_optional(&mut **tx)
        .await?
    {
        let old: Manifest = serde_json::from_value(row.get("manifest"))
            .map_err(|_| bad("Invalid prior package"))?;
        let version = |s: &str| {
            s.split('.')
                .map(|v| v.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        };
        if version(&m.version) < version(&old.version) {
            return Err(conflict(
                "Downgrade requires an explicit compatible migration",
            ));
        }
        for e in old.entities {
            let next = m
                .entities
                .iter()
                .find(|v| v.name == e.name)
                .ok_or(conflict("Removing entities requires an explicit migration"))?;
            for f in e.fields {
                if !next.fields.iter().any(|v| {
                    v.name == f.name
                        && v.kind == f.kind
                        && v.required == f.required
                        && v.references == f.references
                        && v.translatable == f.translatable
                }) {
                    return Err(conflict(
                        "Changing/removing fields requires an explicit migration",
                    ));
                }
            }
        }
    }
    let versions = sqlx::query("SELECT manifest FROM app_versions WHERE app=$1")
        .bind(&m.id)
        .fetch_all(&mut **tx)
        .await?;
    for old in versions {
        let old: Manifest =
            serde_json::from_value(old.get("manifest")).map_err(|_| bad("Invalid prior schema"))?;
        for e in old.entities {
            if let Some(next) = m.entities.iter().find(|v| v.name == e.name) {
                for f in e.fields {
                    if let Some(current) = next.fields.iter().find(|v| v.name == f.name)
                        && (current.kind != f.kind
                            || current.required != f.required
                            || current.references != f.references
                            || current.translatable != f.translatable)
                    {
                        return Err(conflict("Shared app column contract cannot change"));
                    }
                }
            }
        }
    }
    for e in &m.entities {
        let name = table(&m.id, &e.name);
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("public.{name}"))
            .fetch_one(&mut **tx)
            .await?;
        let ddl = format!(
            "CREATE TABLE IF NOT EXISTS public.{name}(tenant text NOT NULL REFERENCES public.tenants(id),id text NOT NULL,revision bigint NOT NULL DEFAULT 1,PRIMARY KEY(tenant,id))"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
            .execute(&mut **tx)
            .await?;
        let ddl = format!(
            "ALTER TABLE public.{name} ENABLE ROW LEVEL SECURITY; ALTER TABLE public.{name} FORCE ROW LEVEL SECURITY"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
            .execute(&mut **tx)
            .await?;
        let policy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_policies WHERE schemaname='public' AND tablename=$1 AND policyname='tenant_scope')").bind(&name).fetch_one(&mut **tx).await?;
        if !policy {
            let ddl = format!(
                "CREATE POLICY tenant_scope ON public.{name} USING(tenant=current_setting('rac.tenant',true)) WITH CHECK(tenant=current_setting('rac.tenant',true))"
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
                .execute(&mut **tx)
                .await?;
        }
        for f in &e.fields {
            let present:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND column_name=$2)").bind(&name).bind(&f.name).fetch_one(&mut **tx).await?;
            if exists && !present && f.required {
                return Err(conflict(
                    "New required field needs a data migration; add it nullable first",
                ));
            }
            let kind = if f.translatable {
                "jsonb"
            } else {
                match f.kind.as_str() {
                    "integer" => "bigint",
                    "boolean" => "boolean",
                    _ => "text",
                }
            };
            let ddl = format!(
                "ALTER TABLE public.{name} ADD COLUMN IF NOT EXISTS {} {kind} {}",
                f.name,
                if f.required { "NOT NULL" } else { "" }
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
                .execute(&mut **tx)
                .await?;
            if f.indexed {
                let ddl = format!(
                    "CREATE INDEX IF NOT EXISTS idx_{} ON public.{name}(tenant,{})",
                    &hash(&format!("{name}:{}", f.name))[..20],
                    f.name
                );
                sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    for e in &m.entities {
        for f in &e.fields {
            if let Some(target) = &f.references {
                let name = table(&m.id, &e.name);
                let constraint = format!("fk_{}", &hash(&format!("{name}:{}", f.name))[..20]);
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conname=$1)",
                )
                .bind(&constraint)
                .fetch_one(&mut **tx)
                .await?;
                if !exists {
                    let ddl = format!(
                        "ALTER TABLE public.{name} ADD CONSTRAINT {constraint} FOREIGN KEY(tenant,{}) REFERENCES public.{}(tenant,id)",
                        f.name,
                        table(&m.id, target)
                    );
                    sqlx::raw_sql(sqlx::AssertSqlSafe(ddl.as_str()))
                        .execute(&mut **tx)
                        .await?;
                }
            }
        }
    }
    sqlx::query("INSERT INTO app_versions(tenant,app,version,digest,manifest) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(t).bind(&m.id).bind(&m.version).bind(&digest).bind(&value).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO app_packages(tenant,id,version,manifest,digest) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant,id) DO UPDATE SET version=EXCLUDED.version,manifest=EXCLUDED.manifest,digest=EXCLUDED.digest,active=true,revision=app_packages.revision+1").bind(t).bind(&m.id).bind(&m.version).bind(value).bind(&digest).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.installed',$2)")
        .bind(t)
        .bind(json!({"app":m.id,"version":m.version,"digest":digest}))
        .execute(&mut **tx)
        .await?;
    if let Some(c) = &m.configuration {
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM public.{} WHERE tenant=$1 AND id=$2)",
            table(&m.id, &c.entity)
        );
        sqlx::query("SELECT set_config('rac.tenant',$1,true)")
            .bind(t)
            .execute(&mut **tx)
            .await?;
        let exists: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(t)
            .bind(&c.record)
            .fetch_one(&mut **tx)
            .await?;
        if !exists {
            let entity = m.entities.iter().find(|e| e.name == c.entity).unwrap();
            data::save_tx(
                tx,
                t,
                &m,
                entity,
                &json!({"id":c.record,"fields":c.default_fields}),
            )
            .await?;
        }
    }
    Ok(json!({"installed":true,"id":m.id,"version":m.version,"digest":digest}))
}
