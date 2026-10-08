//! Apply a checked migration under tenant-local DDL locks with bounded recovery history and optimistic record revisions.
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
    let shape = shape(old, m, path)?;
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
        let name = table(t, &m.id, &e.name);
        let sql = format!("LOCK TABLE public.{name} IN ACCESS EXCLUSIVE MODE");
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&mut **tx)
            .await?;
        let sql = format!(
            "SELECT to_jsonb(r) AS row FROM public.{name} r WHERE tenant=$1 ORDER BY id LIMIT 2001"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(t)
            .fetch_all(&mut **tx)
            .await?;
        if rows.len() > 2000 {
            return Err(conflict(
                "Online schema migration is limited to 2000 records per model; use an offline migration for larger data",
            ));
        }
        let before = rows
            .iter()
            .map(|r| r.get::<Value, _>("row"))
            .collect::<Vec<_>>();
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
    for step in &path.steps {
        let (entity, field) = step.keys();
        let name = table(t, &m.id, entity);
        let sql = match step {
            Step::Rename { to, .. } => format!(
                "ALTER TABLE public.{name} RENAME COLUMN {} TO {}",
                column(field),
                column(to)
            ),
            Step::Remove { .. } => {
                format!("ALTER TABLE public.{name} DROP COLUMN {}", column(field))
            }
            Step::Convert { .. } => {
                let f = m
                    .entities
                    .iter()
                    .find(|e| e.name == entity)
                    .unwrap()
                    .fields
                    .iter()
                    .find(|f| f.name == field)
                    .unwrap();
                format!(
                    "ALTER TABLE public.{name} ALTER COLUMN {} DROP NOT NULL, ALTER COLUMN {} TYPE {} USING NULL",
                    column(field),
                    column(field),
                    field_values::sql_kind(f)
                )
            }
            Step::Fill { .. } => {
                let f = m
                    .entities
                    .iter()
                    .find(|e| e.name == entity)
                    .unwrap()
                    .fields
                    .iter()
                    .find(|f| f.name == field)
                    .unwrap();
                format!(
                    "ALTER TABLE public.{name} ADD COLUMN IF NOT EXISTS {} {}",
                    column(field),
                    field_values::sql_kind(f)
                )
            }
        };
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&mut **tx)
            .await?;
    }
    for (entity, rows) in changed {
        let name = table(t, &m.id, &entity);
        let e = m.entities.iter().find(|e| e.name == entity).unwrap();
        let sets = e
            .fields
            .iter()
            .filter(|f| {
                shape
                    .iter()
                    .find(|v| v.name == entity)
                    .unwrap()
                    .fields
                    .iter()
                    .any(|old| old.name == f.name)
            })
            .map(|f| format!("{}=x.{}", column(&f.name), column(&f.name)))
            .chain(std::iter::once("revision=x.revision".into()))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE public.{name} r SET {sets} FROM jsonb_populate_recordset(NULL::public.{name},$2) x WHERE r.tenant=$1 AND x.tenant=$1 AND r.id=x.id"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(t)
            .bind(json!(rows))
            .execute(&mut **tx)
            .await?;
        for f in &e.fields {
            if !shape
                .iter()
                .find(|v| v.name == entity)
                .unwrap()
                .fields
                .iter()
                .any(|old| old.name == f.name)
            {
                continue;
            }
            let sql = format!(
                "ALTER TABLE public.{name} ALTER COLUMN {} {} NOT NULL",
                column(&f.name),
                if f.required { "SET" } else { "DROP" }
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&mut **tx)
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
