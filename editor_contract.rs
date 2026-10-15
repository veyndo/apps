//! Editor mounts, enumerated fields and tenant-owned core references; no app alters core tables.
use super::*;
pub(super) fn validate_fields(e: &Entity) -> Result<()> {
    for f in &e.fields {
        if let Some(kind) = f.core_reference.as_deref()
            && (!matches!(
                kind,
                "product" | "customer" | "order" | "b2bCompany" | "b2bUnit" | "salesChannel"
            ) || f.kind != "string"
                || !f.indexed
                || !f.required
                || f.translatable
                || f.references.is_some()
                || !f.choices.is_empty()
                || !verified_kernel::app_core_reference_admissible(
                    kind == "product",
                    !e.public_read,
                ))
        {
            return Err(bad(
                "Core references need indexed private strings; only products may be public",
            ));
        }
        let mut choices = std::collections::HashSet::new();
        if f.choices.len() > 100
            || (!f.choices.is_empty()
                && (f.kind != "string" || f.translatable || f.references.is_some()))
        {
            return Err(bad("Choices require a plain string field, maximum 100"));
        }
        for c in &f.choices {
            if !identifier(&c.value)
                || !choices.insert(&c.value)
                || c.label.is_empty()
                || c.label.len() > 100
                || c.label
                    .values()
                    .any(|v| v.trim().is_empty() || v.len() > 100)
            {
                return Err(bad(
                    "Choice values must be unique identifiers with translated labels",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn validate_binding(
    m: &Manifest,
    view: &str,
    b: &native_views::NativeBlock,
) -> Result<()> {
    let Some(binding) = &b.context_binding else {
        return Ok(());
    };
    let kind = match binding.key.as_str() {
        "productId" => "product",
        "customerId" => "customer",
        "orderId" => "order",
        "companyId" => "b2bCompany",
        "unitId" => "b2bUnit",
        _ => return Err(bad("Unknown editor context key")),
    };
    if !m.entities.iter().any(|e| {
        Some(&e.name) == b.entity.as_ref()
            && e.fields
                .iter()
                .any(|f| f.name == binding.field && f.core_reference.as_deref() == Some(kind))
    }) {
        return Err(bad(
            "Editor binding must reference the corresponding owned core object",
        ));
    }
    for s in m
        .surfaces
        .iter()
        .filter(|s| s.ui_path == format!("native/{view}"))
    {
        let valid = match kind {
            "product" => [
                "admin.product",
                "admin.product.general",
                "admin.product.tab",
                "product.detail",
            ]
            .contains(&s.location.as_str()),
            "customer" => s.location == "admin.customer",
            "b2bCompany" => s.location == "admin.b2b.company",
            "b2bUnit" => s.location == "admin.b2b.unit",
            _ => ["admin.order", "admin.order.general"].contains(&s.location.as_str()),
        };
        if !valid {
            return Err(bad("Context binding requires a matching editor mount"));
        }
    }
    Ok(())
}
/// The generic entity endpoint cannot bypass stricter action permissions.
pub(crate) fn entity_access(
    m: &Manifest,
    e: &Entity,
    h: &RequestContext,
    handler: &str,
) -> Result<()> {
    if handler == "save"
        && m.actions
            .iter()
            .any(|a| a.handler == "submit" && a.entity.as_deref() == Some(e.name.as_str()))
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Append-only form inbox cannot be edited or restored".into(),
        ));
    }
    for action in m
        .actions
        .iter()
        .filter(|a| a.entity.as_deref() == Some(e.name.as_str()) && a.handler == handler)
    {
        gateway::additional_permissions(h, action)?;
        if let Some(scope) = action.permission.as_deref() {
            auth::permit(h, scope)?;
        }
    }
    Ok(())
}
pub(super) async fn validate_references(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    e: &Entity,
    fields: &Value,
) -> Result<()> {
    for f in &e.fields {
        let Some(kind) = f.core_reference.as_deref() else {
            continue;
        };
        let Some(id) = fields[&f.name].as_str() else {
            continue;
        };
        let query = match kind {
            "product" => "SELECT EXISTS(SELECT 1 FROM products WHERE tenant=$1 AND id=$2)",
            "customer" => "SELECT EXISTS(SELECT 1 FROM customers WHERE tenant=$1 AND id=$2)",
            "salesChannel" => {
                "SELECT EXISTS(SELECT 1 FROM sales_channels WHERE tenant=$1 AND id=$2)"
            }
            "order" => "SELECT EXISTS(SELECT 1 FROM orders WHERE tenant=$1 AND id=$2)",
            "b2bCompany" => "SELECT EXISTS(SELECT 1 FROM b2b_companies WHERE tenant=$1 AND id=$2)",
            "b2bUnit" => "SELECT EXISTS(SELECT 1 FROM b2b_units WHERE tenant=$1 AND id=$2)",
            _ => return Err(bad("Unknown core reference")),
        };
        let owned: bool = sqlx::query_scalar(query)
            .bind(t)
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        if !owned {
            return Err(bad("Referenced core object does not belong to this shop"));
        }
    }
    Ok(())
}
