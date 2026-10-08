//! Bounded recursive action-input contract. Untyped managed fields still receive depth/size budgets.
use super::*;
const DEPTH: usize = 12;
const NODES: usize = 2048;
fn invalid(path: &str, reason: &str) -> Error {
    bad(format!("Action input {path}: {reason}"))
}
fn bounded(v: &Value, depth: usize, left: &mut usize, path: &str) -> Result<()> {
    if depth > DEPTH || *left == 0 {
        return Err(invalid(path, "nested input budget exceeded"));
    }
    *left -= 1;
    match v {
        Value::String(s) if s.len() > 8192 => {
            return Err(invalid(path, "string exceeds 8192 bytes"));
        }
        Value::Array(values) => {
            if values.len() > 1000 {
                return Err(invalid(path, "array exceeds 1000 entries"));
            }
            for (i, value) in values.iter().enumerate() {
                bounded(value, depth + 1, left, &format!("{path}[{i}]"))?;
            }
        }
        Value::Object(values) => {
            if values.len() > 100 || values.keys().any(|k| k.len() > 100) {
                return Err(invalid(path, "object exceeds field budget"));
            }
            for (key, value) in values {
                bounded(value, depth + 1, left, &format!("{path}.{key}"))?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn node(schema: &Value, v: &Value, path: &str) -> Result<()> {
    let valid = match schema["type"].as_str() {
        Some("object") => v.is_object(),
        Some("array") => v.is_array(),
        Some("string") => v.is_string(),
        Some("integer") => v.as_i64().is_some(),
        Some("number") => v.as_f64().is_some_and(f64::is_finite),
        Some("boolean") => v.is_boolean(),
        Some("null") => v.is_null(),
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
                node(&schema["items"], v, &format!("{path}[{i}]"))?;
            }
        }
    }
    if let Some(values) = v.as_object() {
        let props = schema["properties"].as_object();
        if schema["required"].as_array().is_some_and(|keys| {
            keys.iter()
                .any(|k| k.as_str().is_none_or(|s| !values.contains_key(s)))
        }) {
            return Err(invalid(path, "required argument missing"));
        }
        for (key, value) in values {
            if let Some(child) = props.and_then(|p| p.get(key)) {
                node(child, value, &format!("{path}.{key}"))?;
            } else if schema["additionalProperties"] == false {
                return Err(invalid(&format!("{path}.{key}"), "unknown argument"));
            } else if schema["additionalProperties"].is_object() {
                node(
                    &schema["additionalProperties"],
                    value,
                    &format!("{path}.{key}"),
                )?;
            }
        }
    }
    Ok(())
}
pub(crate) fn validate(schema: &Value, v: &Value) -> Result<()> {
    if !v.is_object() || v.to_string().len() > 65536 {
        return Err(bad("Action input must be an object of at most 64 KiB"));
    }
    let mut nodes = NODES;
    bounded(v, 0, &mut nodes, "$")?;
    node(schema, v, "$")
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
        if s.get("type").is_some_and(|v| !v.is_string())
            || s.get("properties").is_some_and(|v| !v.is_object())
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
        if s["type"] == "array" && !s["items"].is_object() {
            return Err(bad("Arrays require an item schema"));
        }
        Ok(())
    }
    walk(schema, 0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_types_choices_and_bounds_are_enforced() {
        let s = json!({"type":"object","additionalProperties":false,"properties":{"items":{"type":"array","maxItems":2,"items":{"type":"object","additionalProperties":false,"required":["price"],"properties":{"price":{"type":"number","minimum":0,"maximum":5},"name":{"type":"string","maxLength":3}}}}}});
        assert!(validate(&s, &json!({"items":[{"price":1.25,"name":"abc"}]})).is_ok());
        for value in [
            json!({"items":[{"price":"1"}]}),
            json!({"items":[{"price":-1}]}),
            json!({"items":[{"price":1,"secret":true}]}),
            json!({"items":[{"price":1,"name":"long"}]}),
            json!({"items":[{"price":1},{"price":1},{"price":1}]}),
        ] {
            assert!(validate(&s, &value).is_err());
        }
    }
    #[test]
    fn unsupported_or_malformed_constraints_fail_closed() {
        for s in [
            json!({"type":"string","maxLength":"5"}),
            json!({"type":"number","minimum":5,"maximum":2}),
            json!({"type":"integer","exclusiveMinimum":0}),
            json!({"type":"object","required":["missing"]}),
            json!({"type":"array","items":{"type":"integer"},"uniqueItems":"yes"}),
        ] {
            assert!(validate_contract(&s).is_err(), "{s}");
        }
    }
    #[test]
    fn open_managed_fields_remain_bounded() {
        let s = json!({"type":"object","properties":{"fields":{"type":"object"}}});
        let mut v = json!(0);
        for _ in 0..14 {
            v = json!({"nested":v});
        }
        assert!(validate(&s, &json!({"fields":v})).is_err());
        assert!(validate(&s, &json!({"fields":{"title":"x".repeat(8193)}})).is_err());
    }
}
