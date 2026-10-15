//! Managed app tables: typed writes, optimistic revisions, bounded reads and local RLS context.
use super::*;
pub(crate) fn fields(e: &Entity, v: &Value) -> Result<()> {
    field_values::validate(e, v)
}
/// Keyset pages and indexed equality filters never hydrate an app's entire table.
pub(crate) async fn list_page(
    a: &App,
    t: &str,
    m: &Manifest,
    e: &Entity,
    input: &Value,
) -> Result<Value> {
    list_scoped(a, t, m, e, input, None).await
}
pub(super) async fn list_scoped(
    a: &App,
    t: &str,
    m: &Manifest,
    e: &Entity,
    input: &Value,
    public_channel: Option<&str>,
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
        .map(|f| crate::apps::column(&f.name))
        .collect::<Vec<_>>()
        .join(",");
    let prefix = format!(
        "SELECT to_jsonb(r) AS data FROM (SELECT id,revision,{columns} FROM public.{} WHERE tenant=",
        table(t, &m.id, &e.name)
    );
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(prefix);
    query.push_bind(t).push(" AND id > ").push_bind(after);
    if let Some(id) = input.get("id") {
        let id = id
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 100)
            .ok_or(bad("Invalid app record ID"))?;
        query.push(" AND id = ").push_bind(id);
    }
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
                .push(format!(" AND {} = ", column(&f.name)))
                .push_bind(text)
                .push(format!("::{kind}"));
        }
    }
    if let Some(channel) = public_channel {
        publication::predicate(&mut query, e, channel);
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
    let mut result =
        json!({"elements":elements,"limit":limit,"hasMore":has_more,"nextCursor":next});
    if let Some(graph) = ontology::project(t, m, e, &result["elements"]) {
        result["ontology"] = graph;
    }
    Ok(result)
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
        table(t, &m.id, &e.name)
    );
    let revision = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(t)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(revision.unwrap_or(0))
}
pub(crate) async fn save(
    a: &App,
    t: &str,
    m: &Manifest,
    e: &Entity,
    v: &Value,
    h: &RequestContext,
) -> Result<Value> {
    let mut tx = a.db.begin().await?;
    history::context(&mut tx, h, "app.save").await?;
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
    super::lock_package_write(tx, t, m).await?;
    let normalized;
    let v = if m.id == "site_builder" {
        normalized = json!({"id":v["id"],"revision":v["revision"],"fields":site_builder::normalize(&v["fields"])?});
        &normalized
    } else {
        v
    };
    fields(e, &v["fields"])?;
    if m.id == "site_builder" {
        site_builder::validate(&e.name, &v["fields"])?;
        site_builder::validate_owned(tx, t, &v["fields"], v["id"].as_str().unwrap_or("")).await?;
    }
    super::editor_contract::validate_references(tx, t, e, &v["fields"]).await?;
    field_values::references(tx, t, e, &v["fields"]).await?;
    if e.fields
        .iter()
        .any(|f| f.translatable || f.kind == "richtext")
    {
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
    let name = table(t, &m.id, &e.name);
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(t)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,8))")
        .bind(format!("{t}:{name}:{id}"))
        .execute(&mut **tx)
        .await?;
    let sql =
        format!("SELECT to_jsonb(r) FROM public.{name} r WHERE tenant=$1 AND id=$2 FOR UPDATE");
    let current: Option<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(t)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    let revision = current
        .as_ref()
        .and_then(|r| r["revision"].as_i64())
        .unwrap_or(0);
    if revision != expected {
        return Err(Error::revision(revision, "App record revision changed"));
    }
    record_lifecycle::change(e, current.as_ref(), &v["fields"])?;
    if let Some(before) = &current {
        let prior: Value = e
            .fields
            .iter()
            .map(|f| (f.name.clone(), before[&f.name].clone()))
            .collect();
        record_history::capture(tx, t, m, e, id, expected, &prior).await?;
    }
    let names = e.fields.iter().map(|f| column(&f.name)).collect::<Vec<_>>();
    let expressions = e
        .fields
        .iter()
        .map(|f| {
            if field_values::json(f) {
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
    record_history::capture(tx, t, m, e, id, revision, &v["fields"]).await?;
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
            publication: None,
            workflow: None,
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
                unique: false,
                validation: None,
            }],
        };
        assert!(fields(&e, &json!({"fee":200})).is_ok());
        assert!(fields(&e, &json!({"fee":"200"})).is_err());
        assert!(fields(&e, &json!({"fee":200,"tenant":"other"})).is_err());
    }
}
