//! Narrow app callbacks delegate to the existing commerce owners and project PII only with separate consent.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new().secure_route(
        "/api/apps/{id}/core/{operation}",
        &[("POST", "read")],
        post(invoke),
    )
}
async fn invoke(
    State(a): State<App>,
    h: RequestContext,
    Path((id, operation)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let start = std::time::Instant::now();
    let result = execute(&a, &h, &id, &operation, v.clone()).await;
    let status = result
        .as_ref()
        .map(|_| StatusCode::OK)
        .unwrap_or_else(|e| e.0);
    observability::record(
        &a,
        &h,
        &id,
        &format!("core_{operation}"),
        &v,
        status,
        start.elapsed().as_millis(),
    )
    .await;
    result.map(Json)
}
async fn execute(
    a: &App,
    h: &RequestContext,
    id: &str,
    operation: &str,
    v: Value,
) -> Result<Value> {
    let t = merchant(a, h)?;
    if h.principal.app.as_deref().is_some_and(|s| s != id) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "App callback identity mismatch".into(),
        ));
    }
    if h.principal.app.is_none() {
        auth::permit(h, "apps.manage")?;
    }
    let m = package(a, &t, id, true).await?;
    if ["assets", "asset", "asset_preview"].contains(&operation) {
        if !m.permissions.iter().any(|p| p == "assets.read") {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "App asset read permission required".into(),
            ));
        }
        input_schema::validate(
            &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"productId":{"type":"string","maxLength":100},"after":{"type":"string","maxLength":100}},"additionalProperties":false}),
            &v,
        )?;
        credentials::permit(h, "assets.read")?;
        return assets::app_file_callback(a, h, operation, &v).await;
    }
    if ["job_claim", "job_progress"].contains(&operation) {
        if !m.permissions.iter().any(|p| p == "jobs.write") {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "App job permission required".into(),
            ));
        }
        credentials::permit(h, "jobs.write")?;
        auth::permit(h, "apps.manage")?;
        return job_callbacks::execute(a, h, &m, operation, &v).await;
    }
    if operation == "receipt_snapshot" {
        input_schema::validate(
            &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"receiptId":{"type":"string","maxLength":100}},"required":["id","receiptId"],"additionalProperties":false}),
            &v,
        )?;
        for permission in ["documents.read", "orders.read", "customers.pii"] {
            if !m.permissions.iter().any(|p| p == permission) {
                return Err(Error(
                    StatusCode::FORBIDDEN,
                    "Receipt export requires explicit document, order and PII consent".into(),
                ));
            }
            credentials::permit(h, permission)?;
            auth::permit(h, credentials::scope(permission).unwrap())?;
        }
        return crate::operations::receipt_snapshot(
            a,
            h,
            v["id"].as_str().unwrap(),
            v["receiptId"].as_str().unwrap(),
        )
        .await;
    }
    if operation == "record_action" {
        input_schema::validate(
            &json!({"type":"object","properties":{"action":{"type":"string","maxLength":32},"input":{"type":"object"}},"required":["action","input"],"additionalProperties":false}),
            &v,
        )?;
        let name = v["action"].as_str().ok_or(bad("Action required"))?;
        let action = m
            .actions
            .iter()
            .find(|action| {
                action.name == name && ["list", "save"].contains(&action.handler.as_str())
            })
            .ok_or(bad("Callback requires a declared native list/save action"))?;
        let permission = if action.handler == "list" {
            "data.read"
        } else {
            "data.write"
        };
        if !m.permissions.iter().any(|p| p == permission) {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Native record capability not declared".into(),
            ));
        }
        credentials::permit(h, permission)?;
        return invoke_app(a, h, id, name, &v["input"]).await;
    }
    if ["order_import", "channel", "channel_product"].contains(&operation) {
        return core_exchange::execute(a, h, &m, operation, &v).await;
    }
    if operation == "b2b_requisition" {
        if !m.permissions.iter().any(|p| p == "b2b.manage") {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "App B2B permission required".into(),
            ));
        }
        credentials::permit(h, "b2b.manage")?;
        return crate::b2b::invoke(a, h, "b2b.requisition.import", &v).await;
    }
    let (capability, command) = match operation {
        "order" => ("orders.read", "merchant.order"),
        "customer" => ("customers.read", "merchant.customer"),
        "product" => ("products.read", "merchant.product.content"),
        "product_save" => ("products.write", "merchant.product.save"),
        _ => return Err(Error(StatusCode::NOT_FOUND, "Unknown core callback".into())),
    };
    if !m.permissions.iter().any(|p| p == capability) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "App does not declare this core permission".into(),
        ));
    }
    credentials::permit(h, capability)?;
    auth::permit(h, credentials::scope(capability).unwrap())?;
    input_schema::validate(
        &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"product":{"type":"object"}},"required":["id"],"additionalProperties":false}),
        &v,
    )?;
    let mut arguments = v;
    if operation == "customer" {
        let email: Option<String> =
            sqlx::query_scalar("SELECT email FROM customers WHERE tenant=$1 AND id=$2")
                .bind(&t)
                .bind(arguments["id"].as_str())
                .fetch_optional(&a.db)
                .await?;
        arguments["id"] =
            json!(email.ok_or(Error(StatusCode::NOT_FOUND, "Customer not found".into()))?);
    }
    let pii = credentials::permit(h, "customers.pii").is_ok()
        && m.permissions.iter().any(|p| p == "customers.pii")
        && auth::permit(h, "customers.pii").is_ok();
    if !pii && ["order", "customer"].contains(&operation) {
        return crate::operations::redacted_read(
            a,
            h,
            operation,
            arguments["id"].as_str().unwrap(),
        )
        .await;
    }
    crate::operations::invoke(a, h, command, &arguments).await
}
