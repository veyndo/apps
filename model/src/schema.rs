//! Bounded recursive action-input contract. Untyped managed fields still receive depth/size budgets.
use crate::{Error, Result};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
#[derive(Clone, Copy)]
struct Budget {
    depth: usize,
    nodes: usize,
    string: usize,
    fields: usize,
}
const APP: Budget = Budget {
    depth: 12,
    nodes: 2048,
    string: 8192,
    fields: 100,
};
// Native rich documents count content/array/mark wrappers as well as domain nodes.
// MCP still has the same 64 KiB request cap; domain validation remains authoritative.
const NATIVE: Budget = Budget {
    depth: 32,
    nodes: 16384,
    string: 65536,
    fields: 1000,
};
fn invalid(path: &str, reason: &str) -> Error {
    Error::validation(path, format!("Action input {path}: {reason}"))
}
fn pointer_child(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn matches_type(kind: &str, v: &Value) -> bool {
    match kind {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.as_i64().is_some(),
        "number" => v.as_f64().is_some_and(f64::is_finite),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => false,
    }
}
fn bounded(v: &Value, depth: usize, left: &mut usize, path: &str, budget: Budget) -> Result<()> {
    if depth > budget.depth || *left == 0 {
        return Err(invalid(path, "nested input budget exceeded"));
    }
    *left -= 1;
    match v {
        Value::String(s) if s.len() > budget.string => {
            return Err(invalid(path, "string exceeds byte budget"));
        }
        Value::Array(values) => {
            if values.len() > 1000 {
                return Err(invalid(path, "array exceeds 1000 entries"));
            }
            for (i, value) in values.iter().enumerate() {
                bounded(
                    value,
                    depth + 1,
                    left,
                    &pointer_child(path, &i.to_string()),
                    budget,
                )?;
            }
        }
        Value::Object(values) => {
            if values.len() > budget.fields || values.keys().any(|k| k.len() > 100) {
                return Err(invalid(path, "object exceeds field budget"));
            }
            for (key, value) in values {
                bounded(value, depth + 1, left, &pointer_child(path, key), budget)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn node(schema: &Value, v: &Value, path: &str) -> Result<()> {
    let valid = match schema.get("type") {
        Some(Value::String(kind)) => matches_type(kind, v),
        Some(Value::Array(kinds)) => kinds
            .iter()
            .any(|kind| kind.as_str().is_some_and(|kind| matches_type(kind, v))),
        None => true,
        _ => false,
    };
    if !valid {
        return Err(invalid(path, "type mismatch"));
    }
    if let Some(values) = schema["enum"].as_array()
        && !values.contains(v)
    {
        return Err(invalid(path, "value is not an allowed choice"));
    }
    if let Some(n) = v.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|min| n < min)
            || schema["maximum"].as_f64().is_some_and(|max| n > max))
    {
        return Err(invalid(path, "number outside declared bounds"));
    }
    if let Some(s) = v.as_str() {
        let len = s.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|min| len < min)
            || schema["maxLength"].as_u64().is_some_and(|max| len > max)
        {
            return Err(invalid(path, "text length outside declared bounds"));
        }
    }
    if let Some(values) = v.as_array() {
        let len = values.len() as u64;
        if schema["minItems"].as_u64().is_some_and(|min| len < min)
            || schema["maxItems"].as_u64().is_some_and(|max| len > max)
        {
            return Err(invalid(path, "array length outside declared bounds"));
        }
        if schema["uniqueItems"] == true
            && values
                .iter()
                .enumerate()
                .any(|(i, v)| values[..i].contains(v))
        {
            return Err(invalid(path, "duplicate array entry"));
        }
        if schema["items"].is_object() {
            for (i, v) in values.iter().enumerate() {
                node(&schema["items"], v, &pointer_child(path, &i.to_string()))?;
            }
        }
    }
    if let Some(values) = v.as_object() {
        let props = schema["properties"].as_object();
        if let Some(keys) = schema["required"].as_array() {
            for key in keys {
                let key = key
                    .as_str()
                    .ok_or_else(|| invalid(path, "invalid required field contract"))?;
                if !values.contains_key(key) {
                    return Err(invalid(
                        &pointer_child(path, key),
                        "required argument missing",
                    ));
                }
            }
        }
        for (key, value) in values {
            if let Some(child) = props.and_then(|p| p.get(key)) {
                node(child, value, &pointer_child(path, key))?;
            } else if schema["additionalProperties"] == false {
                return Err(invalid(&pointer_child(path, key), "unknown argument"));
            } else if schema["additionalProperties"].is_object() {
                node(
                    &schema["additionalProperties"],
                    value,
                    &pointer_child(path, key),
                )?;
            }
        }
    }
    Ok(())
}
pub fn validate_native(schema: &Value, v: &Value) -> Result<()> {
    if !v.is_object() {
        return Err(invalid("", "must be an object"));
    }
    validate_with_budget(schema, v, NATIVE)
}
pub fn validate(schema: &Value, v: &Value) -> Result<()> {
    if !v.is_object() || v.to_string().len() > 65536 {
        return Err(invalid("", "must be an object of at most 64 KiB"));
    }
    validate_value(schema, v)
}
pub fn validate_value(schema: &Value, v: &Value) -> Result<()> {
    validate_with_budget(schema, v, APP)
}
fn validate_with_budget(schema: &Value, v: &Value, budget: Budget) -> Result<()> {
    if v.to_string().len() > 65536 {
        return Err(invalid("", "value exceeds 64 KiB"));
    }
    let mut nodes = budget.nodes;
    bounded(v, 0, &mut nodes, "", budget)?;
    node(schema, v, "")
}

pub use crate::schema_contract::validate_contract;
#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
