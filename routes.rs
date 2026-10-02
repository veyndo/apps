//! Tenant-scoped package lifecycle, generated data endpoints and a shared action adapter.
use super::*;
pub(crate) fn app_router() -> Router<App> {
    Router::new()
        .route("/api/apps", get(app_list).post(app_install))
        .route("/api/apps/{id}", axum::routing::put(app_state))
        .route("/api/apps/{id}/actions/{action}", post(app_action))
        .route("/store-api/apps/{id}/actions/{action}", post(app_action))
        .route(
            "/api/apps/{id}/entities/{entity}",
            get(entity_list).post(entity_save),
        )
        .route("/store-api/apps/slots", get(slots))
        .route("/store-api/apps/{id}/configure", post(configure_action))
}
async fn app_list(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let rows=sqlx::query("SELECT id,version,manifest,active,revision,digest FROM app_packages WHERE tenant=$1 ORDER BY id").bind(t).fetch_all(&a.db).await?;
    Ok(Json(
        json!({"apiVersion":"1","packages":rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"version":r.get::<String,_>("version"),"manifest":r.get::<Value,_>("manifest"),"uiUrl":gateway::ui_url(&r.get::<String,_>("id")),"active":r.get::<bool,_>("active"),"revision":r.get::<i64,_>("revision"),"digest":r.get::<String,_>("digest")})).collect::<Vec<_>>(),"builtIns":["engraving","paypal","shopware_payments"],"serviceExecution":"operator-configured external services; no in-process guest code"}),
    ))
}
async fn app_install(
    State(a): State<App>,
    h: HeaderMap,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    auth::permit(&h, "users")?;
    let t = merchant(&a, &h)?;
    let text = match v["builtIn"].as_str() {
        Some("engraving") => Some(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        )),
        Some("paypal") => Some(include_str!("../../extensions/apps/paypal/manifest.json")),
        Some("shopware_payments") => Some(include_str!(
            "../../extensions/apps/shopware-payments/manifest.json"
        )),
        _ => None,
    };
    let m: Manifest = if let Some(s) = text {
        serde_json::from_str(s).map_err(|_| bad("Invalid built-in"))?
    } else {
        serde_json::from_value(v["manifest"].clone()).map_err(|e| bad(e.to_string()))?
    };
    if ["engraving", "paypal", "shopware_payments"].contains(&m.id.as_str()) && text.is_none() {
        return Err(bad("Built-in app IDs are reserved"));
    }
    let result = install(&a, &t, m).await?;
    if v["builtIn"] == "paypal" {
        let mut tx = a.db.begin().await?;
        let row = sqlx::query("SELECT data FROM commerce_settings WHERE tenant=$1 FOR UPDATE")
            .bind(&t)
            .fetch_one(&mut *tx)
            .await?;
        let mut config: commerce::Settings =
            serde_json::from_value(row.get("data")).map_err(|_| bad("Invalid settings"))?;
        if !config.payments.iter().any(|p| p.id == "paypal-sandbox") {
            config.payments.push(commerce::Payment {
                id: "paypal-sandbox".into(),
                name: "PayPal Sandbox".into(),
                active: payments::account(&t).is_ok(),
                business_only: false,
                mode: "app".into(),
            });
            sqlx::query("UPDATE commerce_settings SET data=$1,revision=revision+1 WHERE tenant=$2")
                .bind(json!(config))
                .bind(&t)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
    Ok(Json(result))
}
async fn app_state(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    auth::permit(&h, "users")?;
    let t = merchant(&a, &h)?;
    let active = v["active"].as_bool().ok_or(bad("active required"))?;
    let changed=sqlx::query("UPDATE app_packages SET active=$1,revision=revision+1 WHERE tenant=$2 AND id=$3 AND revision=$4").bind(active).bind(t).bind(id).bind(v["revision"].as_i64().ok_or(bad("revision required"))?).execute(&a.db).await?.rows_affected();
    if changed != 1 {
        return Err(conflict("App revision changed"));
    }
    Ok(Json(json!({"active":active,"dataRetained":true})))
}
async fn app_action(
    State(a): State<App>,
    h: HeaderMap,
    Path((id, action)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(Json(invoke_app(&a, &h, &id, &action, &v).await?))
}
async fn entity_list(
    State(a): State<App>,
    h: HeaderMap,
    Path((id, name)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, true).await?;
    let e = m
        .entities
        .iter()
        .find(|e| e.name == name)
        .ok_or(bad("Unknown entity"))?;
    Ok(Json(data::list(&a, &t, &m, e).await?))
}
async fn entity_save(
    State(a): State<App>,
    h: HeaderMap,
    Path((id, name)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    auth::permit(&h, "catalog")?;
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, true).await?;
    let e = m
        .entities
        .iter()
        .find(|e| e.name == name)
        .ok_or(bad("Unknown entity"))?;
    Ok(Json(data::save(&a, &t, &m, e, &v).await?))
}
async fn slots(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let rows = sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active")
        .bind(tenant(&h)?)
        .fetch_all(&a.db)
        .await?;
    let mut slots = vec![];
    for r in rows {
        let m: Manifest =
            serde_json::from_value(r.get("manifest")).map_err(|_| bad("Invalid package"))?;
        if m.permissions.contains(&"storefront.slot".into()) {
            for s in m.slots.iter().filter(|s| s.location == "product.detail") {
                slots.push(json!({"app":m.id,"version":m.version,"slot":s,"configuration":m.configuration.as_ref().map(|c|json!({"inputField":c.input_field,"label":c.label,"hint":c.hint}))}));
            }
        }
    }
    Ok(Json(json!({"slots":slots})))
}
async fn configure_action(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(Json(configure(&a, &h, &id, &v).await?))
}
