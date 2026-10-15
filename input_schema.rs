//! Bounded recursive action-input contract. Untyped managed fields still receive depth/size budgets.
use super::*;
const DEPTH: usize = 12;
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
pub(crate) fn validate_native(schema: &Value, v: &Value) -> Result<()> {
    if !v.is_object() {
        return Err(invalid("", "must be an object"));
    }
    validate_with_budget(schema, v, NATIVE)
}
pub(crate) fn validate(schema: &Value, v: &Value) -> Result<()> {
    if !v.is_object() || v.to_string().len() > 65536 {
        return Err(invalid("", "must be an object of at most 64 KiB"));
    }
    validate_value(schema, v)
}
pub(super) fn validate_value(schema: &Value, v: &Value) -> Result<()> {
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

pub(super) fn validate_contract(schema: &Value) -> Result<()> {
    fn walk(s: &Value, depth: usize) -> Result<()> {
        if depth > DEPTH || !s.is_object() {
            return Err(bad("Invalid or deeply nested action schema"));
        }
        if s["type"].as_str().is_some_and(|t| {
            ![
                "object", "array", "string", "integer", "number", "boolean", "null",
            ]
            .contains(&t)
        }) {
            return Err(bad("Unsupported action schema type"));
        }
        let keys = [
            "type",
            "description",
            "title",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "enum",
            "minimum",
            "maximum",
            "minLength",
            "maxLength",
            "minItems",
            "maxItems",
            "uniqueItems",
        ];
        if s.as_object()
            .unwrap()
            .keys()
            .any(|k| !keys.contains(&k.as_str()))
        {
            return Err(bad("Unsupported action schema constraint"));
        }
        for key in ["minLength", "maxLength", "minItems", "maxItems"] {
            if s.get(key).is_some_and(|v| v.as_u64().is_none()) {
                return Err(bad("Schema lengths must be nonnegative integers"));
            }
        }
        for key in ["minimum", "maximum"] {
            if s.get(key)
                .is_some_and(|v| v.as_f64().is_none_or(|n| !n.is_finite()))
            {
                return Err(bad("Schema numeric bounds must be finite numbers"));
            }
        }
        for (lo, hi) in [
            ("minimum", "maximum"),
            ("minLength", "maxLength"),
            ("minItems", "maxItems"),
        ] {
            if let (Some(lo), Some(hi)) = (s[lo].as_f64(), s[hi].as_f64())
                && lo > hi
            {
                return Err(bad("Schema lower bound exceeds upper bound"));
            }
        }
        if s.get("type").is_some_and(|v| {
            !v.is_string()
                && !v.as_array().is_some_and(|types| {
                    !types.is_empty()
                        && types.len() <= 7
                        && types.iter().enumerate().all(|(i, kind)| {
                            kind.as_str().is_some_and(|kind| {
                                [
                                    "object", "array", "string", "integer", "number", "boolean",
                                    "null",
                                ]
                                .contains(&kind)
                            }) && !types[..i].contains(kind)
                        })
                })
        }) || s.get("properties").is_some_and(|v| !v.is_object())
            || s.get("additionalProperties")
                .is_some_and(|v| !v.is_object() && !v.is_boolean())
            || s.get("uniqueItems").is_some_and(|v| !v.is_boolean())
            || s.get("enum")
                .is_some_and(|v| v.as_array().is_none_or(|a| a.is_empty() || a.len() > 100))
            || s.get("required").is_some_and(|v| {
                v.as_array().is_none_or(|a| {
                    a.len() > 100
                        || a.iter()
                            .any(|k| k.as_str().is_none_or(|k| s["properties"].get(k).is_none()))
                })
            })
        {
            return Err(bad("Malformed action schema constraint"));
        }
        if let Some(p) = s["properties"].as_object() {
            if p.len() > 100 {
                return Err(bad("Action schema field limit"));
            }
            for child in p.values() {
                walk(child, depth + 1)?;
            }
        }
        for key in ["items", "additionalProperties"] {
            if s[key].is_object() {
                walk(&s[key], depth + 1)?;
            }
        }
        if (s["type"] == "array"
            || s["type"]
                .as_array()
                .is_some_and(|types| types.contains(&json!("array"))))
            && !s["items"].is_object()
        {
            return Err(bad("Arrays require an item schema"));
        }
        Ok(())
    }
    walk(schema, 0)
}
#[cfg(test)]
#[path = "input_schema_tests.rs"]
mod tests;
