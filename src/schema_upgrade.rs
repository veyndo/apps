//! Original bounded schema conversion and recovery capture; caller owns package fences, current admission and atomic installation.
use crate::{
    RecordKey, Scope,
    installation::HookFuture,
    model::{
        Entity,
        evolution::{self, Step},
        manifest::Manifest,
    },
    records,
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
#[derive(Debug)]
pub enum UpgradeError<E> {
    Database(sqlx::Error),
    Model(crate::model::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    Host(E),
}
impl<E> From<sqlx::Error> for UpgradeError<E> {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
/// Required for every converted row, on the same transaction. No approving default:
/// hosts validate typed fields, enabled languages, currency and rich-content policy.
pub trait Validation: Sync {
    type Error: Send;
    fn fields<'a>(
        &'a self,
        tx: &'a mut sqlx::PgConnection,
        entity: &'a Entity,
        fields: &'a Value,
    ) -> HookFuture<'a, Self::Error>;
}
fn project(entity: &Entity, row: &Value) -> Value {
    let mut fields = serde_json::Map::new();
    fields.insert("id".into(), row["id"].clone());
    fields.insert("revision".into(), row["revision"].clone());
    for field in &entity.fields {
        fields.insert(field.name.clone(), row[&field.name].clone());
    }
    Value::Object(fields)
}
/// Invoke only within admitted complete installation under its tenant/package fences.
/// Errors require caller rollback. No commit, schema creation or automatic historical rewind.
pub async fn upgrade<V: Validation>(
    tx: &mut Transaction<'_, Postgres>,
    scope: Scope<'_>,
    old: &Manifest,
    m: &Manifest,
    validation: &V,
) -> Result<(), UpgradeError<V::Error>> {
    if old.id != scope.app || m.id != scope.app {
        return Err(UpgradeError::Invalid(
            "Schema package identity must match admitted scope",
        ));
    }
    let t = scope.tenant;
    let path = m
        .schema_migrations
        .iter()
        .find(|p| p.from_version == old.version);
    if old.version == m.version {
        return Ok(());
    }
    evolution::shape(&old.entities, &m.entities, path).map_err(UpgradeError::Model)?;
    let Some(path) = path else {
        return Ok(());
    };
    sqlx::query("SELECT set_config('rac.tenant',$1,true),set_config('lock_timeout','2s',true),set_config('statement_timeout','10s',true)").bind(t).execute(&mut **tx).await?;
    let mut backups = serde_json::Map::new();
    let mut changed = Vec::new();
    for e in &old.entities {
        if !path.steps.iter().any(|s| s.keys().0 == e.name) {
            continue;
        }
        let before: Vec<Value> = records::rows(tx, scope, &e.name, 2001)
            .await?
            .iter()
            .map(|row| project(e, row))
            .collect();
        if before.len() > 2000 {
            return Err(UpgradeError::Conflict(
                "Online schema migration is limited to 2000 records per model; use an offline migration for larger data",
            ));
        }
        backups.insert(e.name.clone(), json!(before));
        let target = m.entities.iter().find(|v| v.name == e.name).unwrap();
        let mut after = before;
        for row in &mut after {
            for step in path.steps.iter().filter(|s| s.keys().0 == e.name) {
                let (_, field) = step.keys();
                match step {
                    Step::Rename { to, .. } => {
                        let v = row
                            .as_object_mut()
                            .unwrap()
                            .remove(field)
                            .unwrap_or(Value::Null);
                        row[to] = v;
                    }
                    Step::Remove { .. } => {
                        row.as_object_mut().unwrap().remove(field);
                    }
                    Step::Fill { value, .. } => {
                        if row[field].is_null() {
                            row[field] = value.clone();
                        }
                    }
                    Step::Convert { .. } => {
                        let f = target.fields.iter().find(|f| f.name == field).unwrap();
                        row[field] =
                            evolution::convert(&row[field], f).map_err(UpgradeError::Model)?;
                    }
                }
            }
            let fields = json!(
                target
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), row[&f.name].clone()))
                    .collect::<serde_json::Map<_, _>>()
            );
            validation
                .fields(tx, target, &fields)
                .await
                .map_err(UpgradeError::Host)?;
            row["revision"] = json!(
                row["revision"]
                    .as_i64()
                    .and_then(|revision| revision.checked_add(1))
                    .ok_or(UpgradeError::Conflict("App record revision exhausted"))?
            );
        }
        changed.push((e.name.clone(), after));
    }
    let backup = json!(backups);
    if backup.to_string().len() > 4_000_000 {
        return Err(UpgradeError::Conflict(
            "Schema recovery snapshot exceeds 4 MB",
        ));
    }
    // Bound retained recovery storage; operator export/retention is explicit, never silent truncation.
    let used:i64=sqlx::query_scalar("SELECT coalesce(sum(octet_length(records::text)),0)::bigint FROM app_schema_history WHERE tenant=$1 AND app=$2").bind(t).bind(&m.id).fetch_one(&mut **tx).await?;
    if used + backup.to_string().len() as i64 > 16_000_000 {
        return Err(UpgradeError::Conflict(
            "Schema recovery quota reached; export and archive history first",
        ));
    }
    sqlx::query("INSERT INTO app_schema_history(tenant,app,to_version,from_version,old_manifest,records) VALUES($1,$2,$3,$4,$5,$6)").bind(t).bind(&m.id).bind(&m.version).bind(&old.version).bind(json!(old)).bind(backup).execute(&mut **tx).await?;
    for (entity, rows) in changed {
        let e = m.entities.iter().find(|e| e.name == entity).unwrap();
        records::define(tx, scope, &m.version, &e.name, &json!(e)).await?;
        for row in rows {
            let fields: Value = e
                .fields
                .iter()
                .map(|f| (f.name.clone(), row[&f.name].clone()))
                .collect();
            records::put(
                tx,
                RecordKey {
                    scope,
                    entity: &entity,
                    id: row["id"].as_str().unwrap(),
                },
                row["revision"].as_i64().unwrap(),
                &fields,
            )
            .await?;
        }
    }
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.schema.migrated',$2)")
        .bind(t)
        .bind(json!({"app":m.id,"fromVersion":old.version,"toVersion":m.version}))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
