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
                    && o.len() <= 100
                    && o.keys().all(|k| {
                        k.len() <= 35 && k.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-')
                    })
                    && o.values()
                        .all(|v| v.is_null() || v.as_str().is_some_and(|s| s.len() <= 2000))
            })
        } else {
            match f.kind.as_str() {
                "integer" => value.as_i64().is_some(),
                "boolean" => value.is_boolean(),
                "json" => {
                    (value.is_object() || value.is_array()) && value.to_string().len() <= 8192
                }
                _ => value.as_str().is_some_and(|s| s.len() <= 2000),
            }
        };
        if !f.choices.is_empty()
            && !f
                .choices
                .iter()
                .any(|c| value.as_str() == Some(c.value.as_str()))
        {
            return Err(bad(format!("Unknown choice for {}", f.name)));
        }
        if !ok {
            return Err(bad(format!("Invalid field {}", f.name)));
        }
    }
    Ok(())
}
/// Keyset pages and indexed equality filters never hydrate an app's entire table.
pub(crate) async fn list_page(
    a: &App,
    t: &str,
    m: &Manifest,
    e: &Entity,
    input: &Value,
) -> Result<Value> {
    if !m.permissions.contains(&"data.read".into()) {
        return Err(Error(StatusCode::FORBIDDEN, "App needs data.read".into()));
    }
    let limit = input["limit"].as_i64().unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(bad("App page limit must be 1..100"));
    }
    let after = input["after"].as_str().unwrap_or("");
    if after.len() > 100 {
        return Err(bad("App cursor exceeds limit"));
    }
    let columns = e
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let prefix = format!(
        "SELECT to_jsonb(r) AS data FROM (SELECT id,revision,{columns} FROM public.{} WHERE tenant=",
        table(&m.id, &e.name)
    );
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(prefix);
    query.push_bind(t).push(" AND id > ").push_bind(after);
    if let Some(filter) = input.get("filter") {
        let filter = filter
            .as_object()
            .ok_or(bad("App filter must be an object"))?;
        if filter.len() > 4 {
            return Err(bad("App filter exceeds limit"));
        }
        for (name, value) in filter {
            let f = e
                .fields
                .iter()
                .find(|f| f.name == *name && f.indexed && !f.translatable)
                .ok_or(bad("Filter requires an indexed field"))?;
            let (kind, text) = match f.kind.as_str() {
                "string" => (
                    "text",
                    value
                        .as_str()
                        .filter(|s| s.len() <= 2000)
                        .ok_or(bad("String filter required"))?
                        .to_owned(),
                ),
                "integer" => (
                    "bigint",
                    value
                        .as_i64()
                        .ok_or(bad("Integer filter required"))?
                        .to_string(),
                ),
                "boolean" => (
                    "boolean",
                    value
                        .as_bool()
                        .ok_or(bad("Boolean filter required"))?
                        .to_string(),
                ),
                _ => return Err(bad("Unsupported app filter")),
            };
            query
                .push(format!(" AND {} = ", f.name))
                .push_bind(text)
                .push(format!("::{kind}"));
        }
    }
    query
        .push(" ORDER BY id LIMIT ")
        .push_bind(limit + 1)
        .push(") r");
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut *tx)
        .await?;
    let rows = query.build().fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let has_more = rows.len() > limit as usize;
    let elements = rows
        .iter()
        .take(limit as usize)
        .map(|r| r.get::<Value, _>("data"))
        .collect::<Vec<_>>();
    let next = if has_more {
        elements.last().and_then(|r| r["id"].as_str())
    } else {
        None
    };
    Ok(json!({"elements":elements,"limit":limit,"hasMore":has_more,"nextCursor":next}))
}
/// A proposal binds one indexed record, including records beyond the first page.
pub(crate) async fn record_revision(
    a: &App,
    t: &str,
    m: &Manifest,
    e: &Entity,
    id: &str,
) -> Result<i64> {
    if !m.permissions.contains(&"data.read".into()) {
        return Err(Error(StatusCode::FORBIDDEN, "App needs data.read".into()));
    }
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut *tx)
        .await?;
    let sql = format!(
        "SELECT revision FROM public.{} WHERE tenant=$1 AND id=$2",
        table(&m.id, &e.name)
    );
    let revision = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(t)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(revision.unwrap_or(0))
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
    super::editor_contract::validate_references(tx, t, e, &v["fields"]).await?;
    if e.fields.iter().any(|f| f.translatable) {
        super::native_data::validate_languages(tx, t, e, &v["fields"]).await?;
    }
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
                    "json" => format!("NULLIF($3->'{}','null'::jsonb)", f.name),
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
                core_reference: None,
                choices: vec![],
            }],
        };
        assert!(fields(&e, &json!({"fee":200})).is_ok());
        assert!(fields(&e, &json!({"fee":"200"})).is_err());
        assert!(fields(&e, &json!({"fee":200,"tenant":"other"})).is_err());
    }
}
