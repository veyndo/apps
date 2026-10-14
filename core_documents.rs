//! Dedicated bounded order-document upload requires exact app capabilities, current actor rights and native receipt ownership.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{id}/core/document_upload",
            &[("POST", "documents.create")],
            post(upload),
        )
        .layer(axum::extract::DefaultBodyLimit::max(
            2 * 1024 * 1024 + 65536,
        ))
}
fn permitted(h: &RequestContext, m: &Manifest, write: bool) -> Result<()> {
    if h.principal.app.as_deref().is_some_and(|app| app != m.id) {
        return Err(Error(StatusCode::FORBIDDEN, "App identity mismatch".into()));
    }
    if h.principal.app.is_none() {
        auth::permit(h, "apps.manage")?;
    }
    let permissions = if write {
        vec!["deliveries.write", "documents.create", "customers.pii"]
    } else {
        vec!["orders.read", "customers.pii"]
    };
    for permission in permissions {
        if !m.permissions.iter().any(|p| p == permission) {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Document or shipment capability not declared".into(),
            ));
        }
        credentials::permit(h, permission)?;
        auth::permit(h, credentials::scope(permission).unwrap())?;
    }
    Ok(())
}
async fn upload(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
    mut parts: axum::extract::Multipart,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, true).await?;
    permitted(&h, &m, true)?;
    let mut metadata = None;
    let mut content = None;
    while let Some(part) = parts
        .next_field()
        .await
        .map_err(|_| bad("Invalid document multipart input"))?
    {
        match part.name() {
            Some("metadata") if metadata.is_none() => {
                let bytes = part
                    .bytes()
                    .await
                    .map_err(|_| bad("Invalid document metadata"))?;
                if bytes.len() > 8192 {
                    return Err(bad("Document metadata limit"));
                }
                let v: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| bad("Invalid document metadata JSON"))?;
                input_schema::validate(
                    &json!({"type":"object","properties":{"orderId":{"type":"string","maxLength":100},"requestKey":{"type":"string","minLength":8,"maxLength":128},"kind":{"type":"string","enum":["shipping_label","return_label"]},"deliveryIndex":{"type":"integer","minimum":0},"allocationDigest":{"type":"string","maxLength":64},"trackingCode":{"type":"string","maxLength":100},"trackingUrl":{"type":"string","maxLength":2000}},"required":["orderId","requestKey","kind","deliveryIndex","allocationDigest"],"additionalProperties":false}),
                    &v,
                )?;
                metadata = Some(v);
            }
            Some("file") if content.is_none() => {
                if part.content_type() != Some("application/pdf") {
                    return Err(bad("Carrier documents must use application/pdf"));
                }
                content = Some(
                    part.bytes()
                        .await
                        .map_err(|_| bad("Invalid PDF upload"))?
                        .to_vec(),
                );
            }
            _ => return Err(bad("Duplicate or unknown document multipart field")),
        }
    }
    crate::operations::import_document(
        &a,
        &h,
        &m,
        metadata.ok_or(bad("Document metadata required"))?,
        content.ok_or(bad("PDF required"))?,
    )
    .await
    .map(Json)
}
pub(super) async fn snapshot(
    a: &App,
    h: &RequestContext,
    m: &Manifest,
    v: &Value,
) -> Result<Value> {
    permitted(h, m, false)?;
    input_schema::validate(
        &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"deliveryIndex":{"type":"integer","minimum":0}},"required":["id","deliveryIndex"],"additionalProperties":false}),
        v,
    )?;
    let order = crate::operations::invoke(a, h, "merchant.order", &json!({"id":v["id"]})).await?;
    let index = v["deliveryIndex"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(bad("Shipment index required"))?;
    let delivery = order["deliveries"]
        .as_array()
        .and_then(|v| v.get(index))
        .ok_or(bad("Shipment unavailable"))?;
    Ok(
        json!({"orderId":order["id"],"orderNumber":order["orderNumber"],"orderRevision":order["revision"],"deliveryIndex":index,"allocationDigest":crate::operations::allocation_digest(&order,index),"delivery":delivery,"address":order["shippingAddress"]}),
    )
}
pub(super) async fn booking(
    a: &App,
    h: &RequestContext,
    m: &Manifest,
    operation: &str,
    v: &Value,
) -> Result<Value> {
    permitted(h, m, true)?;
    input_schema::validate(
        &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"deliveryIndex":{"type":"integer","minimum":0},"allocationDigest":{"type":"string","maxLength":64},"requestKey":{"type":"string","minLength":8,"maxLength":128}},"required":["id","deliveryIndex","allocationDigest","requestKey"],"additionalProperties":false}),
        v,
    )?;
    crate::operations::booking_intent(a, h, m, operation, v).await
}
