//! Apply a checked migration under tenant/app package fences with bounded recovery history and optimistic record revisions.
use super::schema_changes::{Step, convert, shape};
use super::*;
pub(super) async fn upgrade(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    old: &Manifest,
    m: &Manifest,
) -> Result<()> {
    let path = m
        .schema_migrations
        .iter()
        .find(|p| p.from_version == old.version);
    if old.version == m.version {
        return Ok(());
    }
    shape(old, m, path)?;
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
        let before = records::rows(tx, t, &m.id, e, 2001).await?;
        if before.len() > 2000 {
            return Err(conflict(
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
                        row[field] = convert(&row[field], f)?;
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
            data::fields(target, &fields)?;
            native_data::validate_languages(tx, t, target, &fields).await?;
            row["revision"] = json!(row["revision"].as_i64().unwrap() + 1);
        }
        changed.push((e.name.clone(), after));
    }
    let backup = json!(backups);
    if backup.to_string().len() > 4_000_000 {
        return Err(conflict("Schema recovery snapshot exceeds 4 MB"));
    }
    // Bound retained recovery storage; operator export/retention is explicit, never silent truncation.
    let used:i64=sqlx::query_scalar("SELECT coalesce(sum(octet_length(records::text)),0)::bigint FROM app_schema_history WHERE tenant=$1 AND app=$2").bind(t).bind(&m.id).fetch_one(&mut **tx).await?;
    if used + backup.to_string().len() as i64 > 16_000_000 {
        return Err(conflict(
            "Schema recovery quota reached; export and archive history first",
        ));
    }
    sqlx::query("INSERT INTO app_schema_history(tenant,app,to_version,from_version,old_manifest,records) VALUES($1,$2,$3,$4,$5,$6)").bind(t).bind(&m.id).bind(&m.version).bind(&old.version).bind(json!(old)).bind(backup).execute(&mut **tx).await?;
    for (entity, rows) in changed {
        let e = m.entities.iter().find(|e| e.name == entity).unwrap();
        records::define(tx, t, &m.id, &m.version, e).await?;
        for row in rows {
            let fields: Value = e
                .fields
                .iter()
                .map(|f| (f.name.clone(), row[&f.name].clone()))
                .collect();
            records::put(
                tx,
                t,
                &m.id,
                &entity,
                row["id"].as_str().unwrap(),
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
