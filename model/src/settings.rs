//! Declared app settings, bounded updates and secret-safe projections; encryption and current authority stay with the host.
use crate::{Result, bad, declarations::identifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Setting {
    pub key: String,
    pub label: HashMap<String, String>,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
}
pub fn validate(settings: &[Setting]) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    if settings.len() > 16 {
        return Err(bad("Maximum 16 app settings"));
    }
    for f in settings {
        if !identifier(&f.key)
            || !names.insert(&f.key)
            || f.label.is_empty()
            || f.label.len() > 100
            || f.label.values().any(|s| s.is_empty() || s.len() > 100)
            || !["string", "secret", "boolean", "integer"].contains(&f.kind.as_str())
        {
            return Err(bad("Invalid app setting declaration"));
        }
    }
    Ok(())
}

/// Only declarations in the admitted current package may reach its runtime.
pub fn declared(settings: &[Setting], values: &Value) -> Value {
    Value::Object(
        settings
            .iter()
            .filter_map(|f| values.get(&f.key).map(|v| (f.key.clone(), v.clone())))
            .collect(),
    )
}

/// Secret values never enter the browser response; configured keys contain no values.
pub fn visible(settings: &[Setting], values: &Value) -> Value {
    Value::Object(
        settings
            .iter()
            .filter(|f| f.kind != "secret")
            .filter_map(|f| values.get(&f.key).map(|v| (f.key.clone(), v.clone())))
            .collect(),
    )
}

pub fn configured(settings: &[Setting], values: &Value) -> Vec<String> {
    settings
        .iter()
        .filter(|f| values.get(&f.key).is_some_and(|v| !v.is_null()))
        .map(|f| f.key.clone())
        .collect()
}

/// Prepare an update only after the host admitted identity, package digest and revision.
/// The result goes to the existing encrypted store, never directly to an API response.
pub fn prepare(settings: &[Setting], current: &Value, input: &Value) -> Result<String> {
    validate(settings)?;
    if !current.is_object() {
        return Err(bad("Stored settings object required"));
    }
    let mut values = declared(settings, current);
    let input = input
        .as_object()
        .filter(|vs| vs.len() <= 16)
        .ok_or(bad("Settings object required"))?;
    for (key, value) in input {
        let f = settings
            .iter()
            .find(|f| f.key == *key)
            .ok_or(bad("Undeclared app setting"))?;
        let valid = match f.kind.as_str() {
            "boolean" => value.is_boolean(),
            "integer" => value.as_i64().is_some(),
            _ => value
                .as_str()
                .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control)),
        };
        if !valid && !value.is_null() {
            return Err(bad("Invalid app setting value"));
        }
        values[key] = value.clone();
    }
    for f in settings {
        if f.required
            && values
                .get(&f.key)
                .is_none_or(|v| v.is_null() || v.as_str().is_some_and(str::is_empty))
        {
            return Err(bad(format!("Required app setting: {}", f.key)));
        }
    }
    let encoded = serde_json::to_string(&values).map_err(|_| bad("Invalid settings"))?;
    if encoded.len() > 16384 {
        return Err(bad("Settings exceed 16 KiB"));
    }
    Ok(encoded)
}
