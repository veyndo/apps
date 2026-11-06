//! Decode immutable accepted graph snapshots and expose labels only; no storage lookup, transition or authority is performed.
use crate::{
    Result, conflict,
    native_lifecycle::{Machine, Snapshot},
};
use serde_json::{Value, json};
use std::collections::HashMap;
pub fn snapshot(data: &Value, object: &str) -> Result<Option<Snapshot>> {
    let Some(value) = data.get("nativeLifecycle") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let s: Snapshot = serde_json::from_value(value.clone())
        .map_err(|_| conflict("Invalid accepted lifecycle; manual recovery required"))?;
    if s.version != 1 || s.object != object {
        return Err(conflict("Accepted lifecycle version mismatch"));
    }
    Ok(Some(s))
}
pub fn order_machine(data: &Value, fallback: Machine, revision: i64) -> Result<(Machine, i64)> {
    Ok(snapshot(data, "order")?
        .map(|s| (s.machine, s.revision))
        .unwrap_or((fallback, revision)))
}

/// Only declared display labels cross the customer projection; never app policies or grants.
pub fn state_labels(raw: &Value) -> Option<(Value, Value)> {
    snapshot(raw, "order").ok().flatten().map(|s| {
        (
            json!(
                s.machine
                    .states
                    .iter()
                    .map(|s| (&s.id, &s.label))
                    .collect::<HashMap<_, _>>()
            ),
            json!(s.main_locale),
        )
    })
}
pub fn project_labels(raw: &Value, output: &mut Value) {
    if let Some((labels, main)) = state_labels(raw) {
        output["stateLabels"] = labels;
        output["stateMainLocale"] = main;
    }
}
