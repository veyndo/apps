//! Exact app field values reuse commerce money, rich content and tenant-owned assets; no HTML or float decimal coercion.
use crate::{Entity, Field, Result, bad, lifecycle, schema};
use serde_json::{Value, json};
pub const KINDS: &[&str] = &[
    "string",
    "integer",
    "boolean",
    "json",
    "date",
    "datetime",
    "money",
    "decimal",
    "image",
    "file",
    "richtext",
    "relations",
];
pub fn json(f: &Field) -> bool {
    f.translatable || matches!(f.kind.as_str(), "json" | "money" | "richtext" | "relations")
}
pub fn contract(e: &Entity) -> Result<()> {
    lifecycle::contract(e)?;
    for f in &e.fields {
        if f.unique && (json(f) || f.translatable) {
            return Err(bad("Unique fields must use scalar values"));
        }
        if let Some(v) = &f.validation {
            schema::validate_contract(v)?;
        }
        if f.validation
            .as_ref()
            .is_some_and(|v| v.get("type").is_some())
        {
            return Err(bad(
                "Field validation cannot override the declared field type",
            ));
        }
    }
    Ok(())
}
fn decimal(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    !whole.is_empty()
        && whole.len() <= 18
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.len() <= 8
        && fraction.bytes().all(|b| b.is_ascii_digit())
        && (!s.contains('.') || !fraction.is_empty())
}
pub fn value<P: ValuePolicy>(f: &Field, v: &Value, policy: &P) -> bool {
    match f.kind.as_str() {
        "integer" => v.as_i64().is_some(),
        "boolean" => v.is_boolean(),
        "json" => (v.is_object() || v.is_array()) && v.to_string().len() <= 8192,
        "money" => policy.money(v),
        "relations" => policy.relations(v) && (!f.required || !v.as_array().unwrap().is_empty()),
        "richtext" => policy.richtext(v) && v.to_string().len() <= 32768,
        "decimal" => v.as_str().is_some_and(decimal),
        "date" => v.as_str().is_some_and(|s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .is_ok_and(|v| v.format("%Y-%m-%d").to_string() == s)
        }),
        "datetime" => v
            .as_str()
            .is_some_and(|s| s.len() <= 40 && chrono::DateTime::parse_from_rfc3339(s).is_ok()),
        "image" | "file" => v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 100),
        _ => v.as_str().is_some_and(|s| s.len() <= 2000),
    }
}
pub fn validate<P: ValuePolicy>(e: &Entity, v: &Value, policy: &P) -> Result<()> {
    let o = v.as_object().ok_or(bad("fields must be an object"))?;
    let unknown = o
        .keys()
        .filter(|k| !e.fields.iter().any(|f| f.name == **k))
        .take(8)
        .map(|k| k.chars().take(64).collect::<String>())
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(bad(format!(
            "Unknown entity field(s) in {}: {}",
            e.name,
            json!(unknown)
        )));
    }
    for f in &e.fields {
        let v = &v[&f.name];
        if v.is_null() {
            if f.required {
                return Err(bad(format!("{} is required", f.name)));
            }
            continue;
        }
        let ok = if f.translatable {
            v.as_object().is_some_and(|o| {
                !o.is_empty()
                    && o.len() <= 100
                    && o.iter()
                        .all(|(k, v)| policy.locale(k) && (v.is_null() || value(f, v, policy)))
            })
        } else {
            value(f, v, policy)
        };
        if !ok {
            return Err(bad(format!("Invalid {} field {}", f.kind, f.name)));
        }
        if !f.choices.is_empty()
            && !f
                .choices
                .iter()
                .any(|c| v.as_str() == Some(c.value.as_str()))
        {
            return Err(bad(format!("Unknown choice for {}", f.name)));
        }
        if let Some(schema) = &f.validation {
            let values = if f.translatable {
                v.as_object()
                    .unwrap()
                    .values()
                    .filter(|v| !v.is_null())
                    .collect::<Vec<_>>()
            } else {
                vec![v]
            };
            for v in values {
                schema::validate(
                    &json!({"type":"object","properties":{"value":schema}}),
                    &json!({"value":v}),
                )?;
            }
        }
    }
    Ok(())
}
/// Required policies reuse each host's canonical currency/content/reference-shape owners.
pub trait ValuePolicy {
    fn locale(&self, key: &str) -> bool;
    fn money(&self, value: &Value) -> bool;
    fn richtext(&self, value: &Value) -> bool;
    fn relations(&self, value: &Value) -> bool;
}
