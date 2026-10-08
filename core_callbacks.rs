//! Narrow app callbacks delegate to the existing commerce owners and project PII only with separate consent.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new().secure_route(
        "/api/apps/{id}/core/{operation}",
        &[("POST", "read")],
        post(invoke),
    )
}
fn projection(value: &Value, keys: &[&str]) -> Value {
    json!(
        keys.iter()
            .filter_map(|k| value.get(*k).map(|v| ((*k).to_owned(), v.clone())))
            .collect::<serde_json::Map<String, Value>>()
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
    let result = crate::operations::invoke(a, h, command, &arguments).await?;
    let pii = credentials::permit(h, "customers.pii").is_ok()
        && m.permissions.iter().any(|p| p == "customers.pii")
        && auth::permit(h, "customers.pii").is_ok();
    let result = if !pii && operation == "order" {
        let mut clean = projection(
            &result,
            &[
                "id",
                "number",
                "orderNumber",
                "state",
                "paymentState",
                "deliveryState",
                "currency",
                "currencyScale",
                "totalMinor",
                "total",
                "createdAt",
            ],
        );
        clean["items"] = json!(
            result["items"]
                .as_array()
                .map(|items| items
                    .iter()
                    .take(100)
                    .map(|item| projection(
                        item,
                        &[
                            "referencedId",
                            "quantity",
                            "price",
                            "unitPrice",
                            "totalPrice"
                        ]
                    ))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
        clean
    } else if !pii && operation == "customer" {
        projection(
            &result,
            &[
                "id",
                "guest",
                "active",
                "groupId",
                "accountType",
                "createdAt",
            ],
        )
    } else {
        result
    };
    Ok(result)
}
