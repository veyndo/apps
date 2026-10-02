//! Managed app tables: typed writes, optimistic revisions, bounded reads and local RLS context.
use super::*;
pub(crate) fn fields(e: &Entity, v: &Value) -> Result<()> {
    let o = v.as_object().ok_or(bad("fields must be an object"))?;
    if o.keys().any(|k| !e.fields.iter().any(|f| f.name == *k)) {
        return Err(bad("Unknown entity field"));
    }
    for f in &e.fields {
        let value = &v[&f.name];
        if value.is_null() {
            if f.required {
                return Err(bad(format!("{} is required", f.name)));
            }
            continue;
        }
        let ok = if f.translatable {
            value.as_object().is_some_and(|o| {
                !o.is_empty()
                    && o.keys()
                        .all(|k| ["en", "de", "fr", "es"].contains(&k.as_str()))
                    && o.values()
                        .all(|v| v.as_str().is_some_and(|s| s.len() <= 2000))
            })
        } else {
            match f.kind.as_str() {
                "integer" => value.as_i64().is_some(),
                "boolean" => value.is_boolean(),
                _ => value.as_str().is_some_and(|s| s.len() <= 2000),
            }
        };
        if !ok {
            return Err(bad(format!("Invalid field {}", f.name)));
        }
    }
    Ok(())
}
pub(crate) async fn list(a: &App, t: &str, m: &Manifest, e: &Entity) -> Result<Value> {
    if !m.permissions.contains(&"data.read".into()) {
        return Err(Error(StatusCode::FORBIDDEN, "App needs data.read".into()));
    }
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut *tx)
        .await?;
    // Select only the installed version's fields; a later tenant's additive columns are private to its contract.
    let columns = e
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT to_jsonb(r) AS data FROM (SELECT id,revision,{columns} FROM public.{} WHERE tenant=$1 ORDER BY id LIMIT 100) r",
        table(&m.id, &e.name)
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(t)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(
        json!({"elements":rows.iter().map(|r|r.get::<Value,_>("data")).collect::<Vec<_>>(),"limit":100}),
    )
}
pub(crate) async fn save(a: &App, t: &str, m: &Manifest, e: &Entity, v: &Value) -> Result<Value> {
    let mut tx = a.db.begin().await?;
    let result = save_tx(&mut tx, t, m, e, v).await?;
    tx.commit().await?;
    Ok(result)
}
pub(crate) async fn save_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
    e: &Entity,
    v: &Value,
) -> Result<Value> {
    if !m.permissions.contains(&"data.write".into()) {
        return Err(Error(StatusCode::FORBIDDEN, "App needs data.write".into()));
    }
    fields(e, &v["fields"])?;
    if let Some(c) = &m.configuration
        && c.entity == e.name
        && let Some(fee) = v["fields"][&c.price_field].as_i64()
    {
        runtime::validate_fee(c, fee).await?;
    }

    let id = v["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 100)
        .ok_or(bad("Record id required"))?;
    let expected = v["revision"].as_i64().unwrap_or(0);
    let name = table(&m.id, &e.name);
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,8))")
        .bind(format!("{t}:{name}:{id}"))
        .execute(&mut **tx)
        .await?;
    let sql = format!("SELECT revision FROM public.{name} WHERE tenant=$1 AND id=$2 FOR UPDATE");
    let current: Option<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(t)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    if current.unwrap_or(0) != expected {
        return Err(conflict("App record revision changed"));
    }
    let names = e.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    let expressions = e
        .fields
        .iter()
        .map(|f| {
            if f.translatable {
                format!("$3->'{}'", f.name)
            } else {
                match f.kind.as_str() {
                    "integer" => format!("($3->>'{}')::bigint", f.name),
                    "boolean" => format!("($3->>'{}')::boolean", f.name),
                    _ => format!("$3->>'{}'", f.name),
                }
            }
        })
        .collect::<Vec<_>>();
    let sets = names
        .iter()
        .map(|n| format!("{n}=EXCLUDED.{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "INSERT INTO public.{name}(tenant,id,{}) VALUES($1,$2,{}) ON CONFLICT(tenant,id) DO UPDATE SET {sets},revision={name}.revision+1 RETURNING revision",
        names.join(","),
        expressions.join(",")
    );
    let revision: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(t)
        .bind(id)
        .bind(&v["fields"])
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.record.changed',$2)")
        .bind(t)
        .bind(json!({"app":m.id,"entity":e.name,"id":id,"revision":revision}))
        .execute(&mut **tx)
        .await?;
    Ok(json!({"id":id,"revision":revision,"fields":v["fields"]}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fields_reject_type_and_unknown_properties() {
        let e = Entity {
            name: "config".into(),
            label: HashMap::new(),
            public_read: false,
            fields: vec![Field {
                name: "fee".into(),
                translatable: false,
                label: HashMap::new(),
                kind: "integer".into(),
                required: true,
                indexed: false,
                references: None,
            }],
        };
        assert!(fields(&e, &json!({"fee":200})).is_ok());
        assert!(fields(&e, &json!({"fee":"200"})).is_err());
        assert!(fields(&e, &json!({"fee":200,"tenant":"other"})).is_err());
    }
}
