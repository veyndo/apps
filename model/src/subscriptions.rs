//! Shared event subscription and minimized payload rules; declared package permissions never replace host current-authority checks.
use crate::{
    Result, bad,
    events::{Context, filtered},
};
use serde_json::{Value, json};
fn permission(m: &Context<'_>, value: &str) -> bool {
    m.permissions.iter().any(|p| p == value)
}
pub fn eligible(m: &Context<'_>, kind: &str, data: &Value) -> bool {
    subscribed(m, kind)
        && (!["app.record.changed", "app.configuration.changed"].contains(&kind)
            || data["app"] == m.id)
        && filtered(m, kind, data)
}
pub fn subscribed(m: &Context<'_>, kind: &str) -> bool {
    let declared = m.events.iter().any(|event| {
        event == kind
            || event
                .strip_suffix(".*")
                .is_some_and(|prefix| kind.starts_with(&format!("{prefix}.")))
    });
    let family = kind.split('.').next().unwrap_or("");
    let scoped =
        permission(m, &format!("events:{kind}")) || permission(m, &format!("events:{family}.*"));
    let own = kind.starts_with(&format!("app.{}.", m.id)) && permission(m, "events:self");
    // Legacy permission retains minimized order/consumer notifications only. Never team/customer/app secrets.
    let legacy = permission(m, "events.read") && ["order", "consumer"].contains(&family);
    declared && (scoped || own || legacy) && (family != "b2b" || permission(m, "b2b.read"))
}
pub fn payload(m: &Context<'_>, kind: &str, data: Value) -> Value {
    if permission(m, "customers.pii")
        && (kind.starts_with("customer.")
            || kind.starts_with("consumer.")
            || kind.starts_with("order.")
            || kind == "payment.instructions.available"
                && permission(m, "payments.instructions.read"))
    {
        return data;
    }
    let mut clean = serde_json::Map::new();
    if let Some(obj) = data.as_object() {
        let keys: &[&str] = if kind.starts_with(&format!("app.{}.", m.id)) {
            return data;
        } else if kind.starts_with("order.") {
            &[
                "orderId",
                "number",
                "totalMinor",
                "currency",
                "currencyScale",
                "state",
                "paymentState",
                "deliveryState",
                "simulation",
                "salesChannelId",
            ]
        } else if kind == "payment.instrument.revoked" {
            &["instrumentId"]
        } else if kind == "payment.instructions.available" {
            &["orderId", "attemptId"]
        } else if kind.starts_with("pos.") {
            &[
                "orderId",
                "salesChannelId",
                "paymentState",
                "totalMinor",
                "currency",
            ]
        } else if kind.starts_with("b2b.") {
            if !permission(m, "b2b.read") {
                return json!({});
            }
            &[
                "id",
                "revision",
                "kind",
                "companyId",
                "unitId",
                "orderId",
                "quoteId",
                "approvalId",
                "state",
                "currency",
                "totalMinor",
            ]
        } else if kind.starts_with("customer.") {
            &["customerId", "id", "revision"]
        } else if kind.starts_with("product.") {
            &["productId", "id", "revision", "stock"]
        } else if kind.starts_with("consumer.") {
            &["requestId", "id", "customerId", "orderId", "kind", "locale"]
        } else if kind.starts_with("delivery.") {
            &[
                "orderId",
                "deliveryId",
                "deliveryIndex",
                "sourceIndex",
                "revision",
                "kind",
                "state",
            ]
        } else if kind == "document.created" {
            &["orderId", "documentId", "kind", "sourceApp"]
        } else if kind.starts_with("intelligence.") || kind == "merchant.change.applied" {
            &[
                "id",
                "claimId",
                "taskId",
                "revision",
                "state",
                "operation",
                "channelId",
                "causalUpliftProven",
                "asOf",
                "autonomous",
            ]
        } else if kind == "app.configuration.changed" {
            &[
                "app",
                "cartId",
                "revision",
                "salesChannelId",
                "operation",
                "products",
            ]
        } else if kind == "app.record.changed" {
            &["app", "entity", "id", "revision"]
        } else {
            &["id", "revision", "kind"]
        };
        for key in keys {
            if let Some(value) = obj.get(*key) {
                clean.insert((*key).into(), value.clone());
            }
        }
    }
    Value::Object(clean)
}
pub fn validate(m: &Context<'_>) -> Result<()> {
    if m.events
        .iter()
        .any(|e| !subscribed(m, e.trim_end_matches(".*")))
    {
        // Check wildcard declarations against a concrete member of the family.
        for e in m.events {
            let concrete = if let Some(prefix) = e.strip_suffix(".*") {
                format!("{prefix}.sample")
            } else {
                e.clone()
            };
            if !subscribed(m, &concrete) {
                return Err(bad(
                    "Event subscription requires a matching events: permission",
                ));
            }
        }
    }
    Ok(())
}
