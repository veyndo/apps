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
    let mut tx = a.db.begin_privileged().await?;
    let result = install_tx(&mut tx, t, m).await?;
    tx.commit().await?;
    Ok(result)
}
pub(crate) async fn install_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: Manifest,
) -> Result<Value> {
    install_inner(tx, t, m, true).await
}
pub(crate) async fn install_preview(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: Manifest,
) -> Result<Value> {
    let private: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM shop_environments WHERE tenant=$1 AND preview_owner IS NOT NULL)").bind(t).fetch_one(&mut **tx).await?;
    if !private {
        return Err(bad(
            "Mutable app preview requires a personal preview environment",
        ));
    }
    install_inner(tx, t, m, false).await
}
async fn install_inner(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: Manifest,
    versions: bool,
) -> Result<Value> {
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut **tx)
        .await?;
    validate(&m)?;
    approval::installation(&m)?;
    commerce_hooks::verify_package(&m).await?;
    crate::marketing::lock_config(tx, t).await?;
    crate::payments::install_methods(tx, t, &m).await?;
    if let Some(c) = &m.configuration {
        let entity = m.entities.iter().find(|e| e.name == c.entity).unwrap();
        data::fields(entity, &c.default_fields)?;
        let fee = c.default_fields[&c.price_field]
            .as_i64()
            .ok_or(bad("Default price required"))?;
        runtime::validate_fee(c, fee).await?;
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,543))")
        .bind(t)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7))")
        .bind(format!("{t}:{}", m.id))
        .execute(&mut **tx)
        .await?;
    // The existing tenant installation lock serializes DDL admission across replicas.
    let admitted: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_packages WHERE tenant=$1 AND id=$2) OR (SELECT count(*) FROM app_packages WHERE tenant=$1)<128").bind(t).bind(&m.id).fetch_one(&mut **tx).await?;
    if !admitted {
        return Err(bad(
            "Maximum 128 registered app packages per shop; update an existing app",
        ));
    }
    distribution::dependencies(tx, t, &m).await?;
    commerce_hooks::admission(tx, t, &m).await?;
    native_lifecycle::admission(tx, t, &m).await?;
    let value = json!(m);
    let digest = hash(&value.to_string());
    if versions
        && let Some(old) =
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
    // Each shop owns logical model contracts; only its own versions constrain upgrades.
    if versions
        && let Some(row) =
            sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2")
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
        schema_upgrade::upgrade(tx, t, &old, &m).await?;
    }
    sqlx::query("INSERT INTO app_packages(tenant,id,version,manifest,digest) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant,id) DO UPDATE SET version=EXCLUDED.version,manifest=EXCLUDED.manifest,digest=EXCLUDED.digest,active=true,revision=app_packages.revision+1").bind(t).bind(&m.id).bind(&m.version).bind(&value).bind(&digest).execute(&mut **tx).await?;
    storage::attach(tx, t, &m).await?;
    records::install(tx, t, &m).await?;
    if versions {
        sqlx::query("INSERT INTO app_versions(tenant,app,version,digest,manifest) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(t).bind(&m.id).bind(&m.version).bind(&digest).bind(&value).execute(&mut **tx).await?;
    }

    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.installed',$2)")
        .bind(t)
        .bind(json!({"app":m.id,"version":m.version,"digest":digest}))
        .execute(&mut **tx)
        .await?;
    if let Some(c) = &m.configuration {
        let exists = records::get(tx, t, &m.id, &c.entity, &c.record, false)
            .await?
            .is_some();
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
    schedules::sync(tx, t, &m).await?;
    Ok(json!({"installed":true,"id":m.id,"version":m.version,"digest":digest}))
}
