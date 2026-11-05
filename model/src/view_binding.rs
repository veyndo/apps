//! Native editor bindings require declared owned core references and corresponding mount locations.
use crate::{
    Result, bad,
    views::{Contract, NativeBlock},
};
pub(crate) fn validate(m: &Contract<'_>, view: &str, b: &NativeBlock) -> Result<()> {
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
