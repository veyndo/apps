//! Bounded app-record history, current entity rights and restoration through the normal managed save path.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{app}/entities/{entity}/{record}/history",
            &[("GET", "read")],
            get(list),
        )
        .secure_route(
            "/api/apps/{app}/entities/{entity}/{record}/history/{revision}/restore",
            &[("POST", "read")],
            post(restore),
        )
}
pub(super) async fn capture(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
    e: &Entity,
    id: &str,
    revision: i64,
    fields: &Value,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,948))")
        .bind(format!("{t}:{}", m.id))
        .execute(&mut **tx)
        .await?;
    let scopes: Vec<String> = m
        .actions
        .iter()
        .filter(|a| a.entity.as_deref() == Some(e.name.as_str()) && a.handler == "list")
        .flat_map(|a| {
            a.permission
                .iter()
                .chain(a.required_permissions.iter())
                .cloned()
        })
        .collect();
    sqlx::query("INSERT INTO app_record_versions(tenant,app,entity,record_id,revision,fields,read_scopes,actor,source) VALUES($1,$2,$3,$4,$5,$6,$7,nullif(current_setting('veyndo.actor',true),''),coalesce(nullif(current_setting('veyndo.source',true),''),'app.save')) ON CONFLICT DO NOTHING")
        .bind(t).bind(&m.id).bind(&e.name).bind(id).bind(revision).bind(fields).bind(json!(scopes)).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM app_record_versions WHERE tenant=$1 AND app=$2 AND entity=$3 AND record_id=$4 AND revision<=$5")
        .bind(t).bind(&m.id).bind(&e.name).bind(id).bind(revision-20).execute(&mut **tx).await?;
    // An app cannot grow retained version storage without bound. Current rows retain their independent quota.
    sqlx::query("DELETE FROM app_record_versions WHERE tenant=$1 AND app=$2 AND (entity,record_id,revision) IN (SELECT entity,record_id,revision FROM (SELECT entity,record_id,revision,sum(pg_column_size(fields)) OVER(ORDER BY created_at DESC,entity,record_id,revision DESC) AS size FROM app_record_versions WHERE tenant=$1 AND app=$2) r WHERE size>16777216)")
        .bind(t).bind(&m.id).execute(&mut **tx).await?;
    Ok(())
}
fn entity<'a>(m: &'a Manifest, name: &str, h: &RequestContext, write: bool) -> Result<&'a Entity> {
    let e = m
        .entities
        .iter()
        .find(|e| e.name == name)
        .ok_or(bad("Unknown app entity"))?;
    auth::permit(h, if write { "apps.manage" } else { "apps.read" })?;
    auth::permit(h, "catalog.read")?;
    editor_contract::entity_access(m, e, h, "list")?;
    if write {
        if !m
            .actions
            .iter()
            .any(|a| a.entity.as_deref() == Some(e.name.as_str()) && a.handler == "save")
        {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "This entity has no restorable save action".into(),
            ));
        }
        editor_contract::entity_access(m, e, h, "save")?;
    }
    Ok(e)
}
async fn list(
    State(a): State<App>,
    h: RequestContext,
    Path((app, name, id)): Path<(String, String, String)>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &app, true).await?;
    let e = entity(&m, &name, &h, false)?;
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    let rows=sqlx::query("SELECT revision,fields,read_scopes,actor,source,created_at::text AS created_at FROM app_record_versions WHERE tenant=$1 AND app=$2 AND entity=$3 AND record_id=$4 ORDER BY revision DESC LIMIT 20")
        .bind(&t).bind(&app).bind(&name).bind(&id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"versions":rows.iter().filter(|r|r.get::<Value,_>("read_scopes").as_array().is_some_and(|scopes|scopes.iter().all(|s|s.as_str().is_some_and(|scope|auth::allowed(&h,scope))))).map(|r|json!({"revision":r.get::<i64,_>("revision"),"fields":e.fields.iter().map(|f|(f.name.clone(),r.get::<Value,_>("fields")[&f.name].clone())).collect::<serde_json::Map<String,Value>>(),"actor":r.get::<Option<String>,_>("actor"),"source":r.get::<String,_>("source"),"createdAt":r.get::<String,_>("created_at")})).collect::<Vec<_>>(),"canRestore":entity(&m,&e.name,&h,true).is_ok()}),
    ))
}
async fn restore(
    State(a): State<App>,
    h: RequestContext,
    Path((app, name, id, revision)): Path<(String, String, String, i64)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &app, true).await?;
    let e = entity(&m, &name, &h, true)?;
    if v["approve"] != true || v["revision"].as_i64().is_none() {
        return Err(bad("Approve restoration with the current revision"));
    }
    let mut tx = a.db.begin().await?;
    history::context(&mut tx, &h, "app.history.restore").await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    let row=sqlx::query("SELECT fields,read_scopes FROM app_record_versions WHERE tenant=$1 AND app=$2 AND entity=$3 AND record_id=$4 AND revision=$5")
        .bind(&t).bind(&app).bind(&name).bind(&id).bind(revision).fetch_optional(&mut *tx).await?.ok_or(Error(StatusCode::NOT_FOUND,"Record version unavailable".into()))?;
    let scopes: Value = row.get("read_scopes");
    if !scopes.as_array().is_some_and(|scopes| {
        scopes
            .iter()
            .all(|s| s.as_str().is_some_and(|scope| auth::allowed(&h, scope)))
    }) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Historical record requires its original read rights".into(),
        ));
    }
    let fields: Value = row.get("fields");
    let result = data::save_tx(
        &mut tx,
        &t,
        &m,
        e,
        &json!({"id":id,"revision":v["revision"],"fields":fields}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(result))
}

/// MCP calls the exact HTTP domain operations, retaining entity and historical read permissions.
pub(crate) fn schema(name: &str) -> Option<Value> {
    if !["merchant.app.history", "merchant.app.restore"].contains(&name) {
        return None;
    }
    Some(
        json!({"type":"object","additionalProperties":false,"properties":{"app":{"type":"string"},"entity":{"type":"string"},"id":{"type":"string"},"version":{"type":"integer"},"revision":{"type":"integer"},"approve":{"type":"boolean"}},"required":if name.ends_with("restore"){vec!["app","entity","id","version","revision","approve"]}else{vec!["app","entity","id"]}}),
    )
}
pub(crate) async fn invoke(a: &App, h: &RequestContext, name: &str, v: &Value) -> Result<Value> {
    let app = v["app"].as_str().ok_or(bad("App required"))?.to_owned();
    let entity = v["entity"]
        .as_str()
        .ok_or(bad("Entity required"))?
        .to_owned();
    let id = v["id"].as_str().ok_or(bad("Record required"))?.to_owned();
    if name == "merchant.app.history" {
        let Json(value) = list(State(a.clone()), h.clone(), Path((app, entity, id))).await?;
        return Ok(value);
    }
    if name != "merchant.app.restore" {
        return Err(bad("Unknown app history operation"));
    }
    let Json(value) = restore(
        State(a.clone()),
        h.clone(),
        Path((
            app,
            entity,
            id,
            v["version"].as_i64().ok_or(bad("Version required"))?,
        )),
        Json(v.clone()),
    )
    .await?;
    Ok(value)
}
