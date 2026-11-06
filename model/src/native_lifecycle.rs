//! Shared native lifecycle admission, locale fallback and narrowing facts; current identity and final native effects remain host decisions.
pub use crate::native_lifecycle_compile::{compile, generation_context, quote_machine};
pub use crate::native_lifecycle_types::*;
use crate::{Result, bad};
use serde_json::Value;
/// Supply complete admitted package metadata. A declared permission or digest does not grant current authority.
pub struct Package<'a> {
    pub id: &'a str,
    pub version: &'a str,
    pub digest: &'a str,
    pub permissions: &'a [String],
    pub lifecycles: &'a [Contract],
}
/// Required host catalogue, locale grammar and native base graph; no default approvals or replacement order model.
pub trait CompilerPolicy {
    fn scopes(&self) -> &[&str];
    fn locale(&self, key: &str) -> bool;
    fn order_base(&self) -> Machine;
}
pub fn validate<P: CompilerPolicy>(m: &Package<'_>, policy: &P) -> Result<()> {
    if m.lifecycles.len() > 2 {
        return Err(bad("At most one order and one quote lifecycle per app"));
    }
    let mut objects = std::collections::HashSet::new();
    for c in m.lifecycles {
        if c.version != 1
            || !["order", "quote"].contains(&c.object.as_str())
            || !objects.insert(&c.object)
            || c.states.is_empty()
            || c.states.len() > 8
            || c.transitions.is_empty()
            || c.transitions.len() > 24
            || !m.permissions.iter().any(|p| p == "commerce.lifecycle")
        {
            return Err(bad(
                "Invalid native lifecycle contract or missing commerce.lifecycle consent",
            ));
        }
        let base = if c.object == "order" {
            policy.order_base()
        } else {
            quote_machine()
        };
        compile(base, std::slice::from_ref(m), &c.object, policy)?;
    }
    Ok(())
}
/// Native displays always fall back to the accepted shop main language, never an arbitrary translation.
pub fn validate_locale(m: &Package<'_>, main: &str) -> Result<()> {
    let base = main.split('-').next().unwrap_or(main);
    for c in m.lifecycles {
        for label in c
            .states
            .iter()
            .map(|s| &s.label)
            .chain(c.transitions.iter().map(|e| &e.label))
        {
            if label
                .get(main)
                .or_else(|| label.get(base))
                .is_none_or(|s| s.trim().is_empty())
            {
                return Err(bad(
                    "App lifecycle labels need the shop main language; other languages inherit",
                ));
            }
        }
    }
    Ok(())
}
pub fn reason(
    edge: &MachineEdge,
    data: &Value,
    permits: impl Fn(&str) -> bool,
) -> Option<&'static str> {
    let Some(policy) = &edge.policy else {
        return None;
    };
    if policy.required_permissions.iter().any(|p| !permits(p)) {
        return Some("lifecyclePermission");
    }
    if policy.guards.iter().any(|fact| !fact_value(fact, data)) {
        return Some("lifecycleGuard");
    }
    None
}
pub fn fact_value(fact: &str, data: &Value) -> bool {
    match fact {
        "payment.ready" => {
            ["paid", "captured"].contains(&data["payment"]["state"].as_str().unwrap_or(""))
                || data["payment"]["provider"] == "simulated"
                    && data["payment"]["state"] == "authorized"
        }
        "delivery.delivered" => data["deliveries"]
            .as_array()
            .is_some_and(|ds| ds.iter().all(|d| d["state"] == "delivered")),
        "quote.message" => data["message"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty()),
        _ => false,
    }
}
