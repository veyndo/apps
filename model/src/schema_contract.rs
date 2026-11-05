//! Closed supported schema grammar, bounds and recursive declaration admission.
use crate::{Result, bad};
use serde_json::{Value, json};
const DEPTH: usize = 12;
pub fn validate_contract(schema: &Value) -> Result<()> {
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
