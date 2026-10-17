//! Versioned declarative native lifecycles; packages narrow native admission and never execute effects.
use super::*;
mod compile;
mod storage;
pub(crate) use compile::{compile, generation_context, quote_machine};
pub(crate) use storage::{
    admission, bind_order, capture, current_order, order_machine, project_labels, snapshot,
    state_labels, validate_active_locale, validate_order_base,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Contract {
    pub version: u8,
    pub object: String,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    pub id: String,
    pub label: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Transition {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Policy {
    pub app: String,
    pub version: String,
    pub digest: String,
    pub required_permissions: Vec<String>,
    pub guards: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub version: u8,
    pub object: String,
    pub revision: i64,
    pub main_locale: String,
    pub machine: commerce::OrderMachine,
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    if m.lifecycles.len() > 2 {
        return Err(bad("At most one order and one quote lifecycle per app"));
    }
    let mut objects = std::collections::HashSet::new();
    for c in &m.lifecycles {
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
            commerce::default_machine()
        } else {
            quote_machine()
        };
        compile(base, std::slice::from_ref(m), &c.object)?;
    }
    Ok(())
}
/// Native displays always fall back to the accepted shop main language, never an arbitrary translation.
pub(crate) fn validate_locale(m: &Manifest, main: &str) -> Result<()> {
    let base = main.split('-').next().unwrap_or(main);
    for c in &m.lifecycles {
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
pub(crate) fn reason(
    h: &RequestContext,
    edge: &commerce::MachineEdge,
    data: &Value,
) -> Option<&'static str> {
    let Some(policy) = &edge.policy else {
        return None;
    };
    if policy
        .required_permissions
        .iter()
        .any(|p| auth::permit(h, p).is_err())
    {
        return Some("lifecyclePermission");
    }
    if policy.guards.iter().any(|fact| !fact_value(fact, data)) {
        return Some("lifecycleGuard");
    }
    None
}
fn fact_value(fact: &str, data: &Value) -> bool {
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
pub(crate) fn allowed(native: bool, rights: bool, guards: bool) -> bool {
    verified_kernel::native_lifecycle_admissible(native, rights, guards)
}

#[cfg(test)]
mod tests;
