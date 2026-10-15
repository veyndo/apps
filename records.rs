//! One fixed PostgreSQL record store; every operation retains tenant/app/model identity and original wire values.
use super::*;
pub(crate) fn lock_key(t: &str, app: &str, entity: &str, id: &str) -> String {
    format!("{t}:{app}:{entity}:{id}")
}
pub(crate) fn project(e: &Entity, row: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("id".into(), row["id"].clone());
    out.insert("revision".into(), row["revision"].clone());
    for f in &e.fields {
        out.insert(f.name.clone(), row[&f.name].clone());
    }
    Value::Object(out)
}
pub(crate) async fn get(
    tx: &mut sqlx::PgConnection,
    t: &str,
    app: &str,
    entity: &str,
    id: &str,
    write: bool,
) -> Result<Option<Value>> {
    let statement = if write {
        "SELECT fields||jsonb_build_object('id',id,'revision',revision) FROM app_records WHERE tenant=$1 AND app=$2 AND entity=$3 AND id=$4 FOR UPDATE"
    } else {
        "SELECT fields||jsonb_build_object('id',id,'revision',revision) FROM app_records WHERE tenant=$1 AND app=$2 AND entity=$3 AND id=$4 FOR SHARE"
    };
    Ok(sqlx::query_scalar(statement)
        .bind(t)
        .bind(app)
        .bind(entity)
        .bind(id)
        .fetch_optional(tx)
        .await?)
}
pub(crate) async fn rows(
    tx: &mut sqlx::PgConnection,
    t: &str,
    app: &str,
    e: &Entity,
    limit: i64,
) -> Result<Vec<Value>> {
    let rows: Vec<Value> = sqlx::query_scalar("SELECT fields||jsonb_build_object('id',id,'revision',revision) FROM app_records WHERE tenant=$1 AND app=$2 AND entity=$3 ORDER BY id LIMIT $4 FOR UPDATE")
        .bind(t).bind(app).bind(&e.name).bind(limit).fetch_all(tx).await?;
    Ok(rows.iter().map(|row| project(e, row)).collect())
}
pub(super) async fn put(
    tx: &mut sqlx::PgConnection,
    t: &str,
    app: &str,
    entity: &str,
    id: &str,
    revision: i64,
    fields: &Value,
) -> Result<()> {
    sqlx::query("INSERT INTO app_records(tenant,app,entity,id,revision,version,fields) VALUES($1,$2,$3,$4,$5,'',$6) ON CONFLICT(tenant,app,entity,id) DO UPDATE SET fields=EXCLUDED.fields,revision=EXCLUDED.revision")
        .bind(t).bind(app).bind(entity).bind(id).bind(revision).bind(fields).execute(tx).await?;
    Ok(())
}
pub(super) async fn define(
    tx: &mut sqlx::PgConnection,
    t: &str,
    app: &str,
    version: &str,
    e: &Entity,
) -> Result<()> {
    sqlx::query("INSERT INTO app_models(tenant,app,entity,version,definition) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant,app,entity) DO UPDATE SET version=EXCLUDED.version,definition=EXCLUDED.definition")
        .bind(t).bind(app).bind(&e.name).bind(version).bind(json!(e)).execute(tx).await?;
    Ok(())
}
pub(super) async fn install(tx: &mut sqlx::PgConnection, t: &str, m: &Manifest) -> Result<()> {
    for e in &m.entities {
        let old: Option<Value> = sqlx::query_scalar(
            "SELECT definition FROM app_models WHERE tenant=$1 AND app=$2 AND entity=$3",
        )
        .bind(t)
        .bind(&m.id)
        .bind(&e.name)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(old) = &old {
            let previous: Entity =
                serde_json::from_value(old.clone()).map_err(|_| bad("Invalid prior app model"))?;
            if e.fields
                .iter()
                .any(|f| f.required && !previous.fields.iter().any(|p| p.name == f.name))
            {
                return Err(conflict(
                    "New required field needs a data migration; add it nullable first",
                ));
            }
        }
        define(tx, t, &m.id, &m.version, e).await?;
        if old.is_some_and(|old| old != json!(e)) {
            // Rebuild bounded fixed projections when field contracts change, never CREATE INDEX/TABLE.
            sqlx::query(
                "UPDATE app_records SET fields=fields WHERE tenant=$1 AND app=$2 AND entity=$3",
            )
            .bind(t)
            .bind(&m.id)
            .bind(&e.name)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}
pub(crate) async fn reset_preview(tx: &mut sqlx::PgConnection, t: &str, app: &str) -> Result<()> {
    // Cyclic links are deferred; erase precisely this private app, never the shared tables.
    sqlx::query("DELETE FROM app_records WHERE tenant=$1 AND app=$2")
        .bind(t)
        .bind(app)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app_models WHERE tenant=$1 AND app=$2")
        .bind(t)
        .bind(app)
        .execute(tx)
        .await?;
    Ok(())
}
