//! Exact app field values reuse commerce money, rich content and tenant-owned assets; no HTML or float decimal coercion.
use super::*;
pub(super) const KINDS: &[&str] = &[
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
pub(super) fn json(f: &Field) -> bool {
    f.translatable || matches!(f.kind.as_str(), "json" | "money" | "richtext" | "relations")
}
pub(super) fn sql_kind(f: &Field) -> &'static str {
    if json(f) {
        "jsonb"
    } else {
        match f.kind.as_str() {
            "integer" => "bigint",
            "boolean" => "boolean",
            _ => "text",
        }
    }
}
pub(super) fn contract(e: &Entity) -> Result<()> {
    for f in &e.fields {
        if f.unique && (json(f) || f.translatable) {
            return Err(bad("Unique fields must use scalar values"));
        }
        if let Some(v) = &f.validation {
            input_schema::validate_contract(v)?;
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
fn money(v: &Value) -> bool {
    let Ok(m) = serde_json::from_value::<vendune::money::Money>(v.clone()) else {
        return false;
    };
    currencies::scale(m.currency().code())==Some(m.currency().scale())
        && v.as_object().is_some_and(|o|o.len()==2)
        && v["currency"].as_object().is_some_and(|o|o.len()==2)
        // JSON clients must not lose precision. Larger amounts require a future string minor-unit wire revision.
        && m.minor().unsigned_abs()<=9_007_199_254_740_991
}
fn value(f: &Field, v: &Value) -> bool {
    match f.kind.as_str() {
        "integer" => v.as_i64().is_some(),
        "boolean" => v.is_boolean(),
        "json" => (v.is_object() || v.is_array()) && v.to_string().len() <= 8192,
        "money" => money(v),
        "relations" => relations::valid(v) && (!f.required || !v.as_array().unwrap().is_empty()),
        "richtext" => assets::validate_rich(v).is_ok() && v.to_string().len() <= 32768,
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
pub(super) fn validate(e: &Entity, v: &Value) -> Result<()> {
    let o = v.as_object().ok_or(bad("fields must be an object"))?;
    if o.keys().any(|k| !e.fields.iter().any(|f| f.name == *k)) {
        return Err(bad("Unknown entity field"));
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
                        .all(|(k, v)| commerce::valid_locale_key(k) && (v.is_null() || value(f, v)))
            })
        } else {
            value(f, v)
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
                input_schema::validate(
                    &json!({"type":"object","properties":{"value":schema}}),
                    &json!({"value":v}),
                )?;
            }
        }
    }
    Ok(())
}
pub(super) async fn references(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    e: &Entity,
    v: &Value,
) -> Result<()> {
    for f in e
        .fields
        .iter()
        .filter(|f| matches!(f.kind.as_str(), "image" | "file"))
    {
        let Some(id) = v[&f.name].as_str() else {
            continue;
        };
        let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_assets WHERE tenant=$1 AND id=$2 AND ($3<> 'image' OR mime LIKE 'image/%') AND (NOT $4 OR public AND kind='attachment'))").bind(t).bind(id).bind(&f.kind).bind(e.public_read).fetch_one(&mut **tx).await?;
        if !owned {
            return Err(bad(
                "App asset is missing, private, or belongs to another shop",
            ));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn field(kind: &str) -> Field {
        serde_json::from_value(json!({"name":"value","kind":kind})).unwrap()
    }
    #[test]
    fn exact_typed_values_reject_coercion_invalid_dates_and_currency_scale() {
        for (kind, v) in [
            ("date", json!("2026-02-29")),
            ("datetime", json!("2026-10-08")),
            ("decimal", json!(0.1)),
            ("decimal", json!("1e3")),
            (
                "money",
                json!({"minor":123,"currency":{"code":"JPY","scale":2}}),
            ),
            (
                "richtext",
                json!({"en":[{"type":"image","url":"javascript:alert(1)"}]}),
            ),
        ] {
            assert!(!value(&field(kind), &v));
        }
        for (kind, v) in [
            ("date", json!("2024-02-29")),
            ("datetime", json!("2026-10-08T12:00:00Z")),
            ("decimal", json!("123456789012345678.12345678")),
            (
                "money",
                json!({"minor":123,"currency":{"code":"JPY","scale":0}}),
            ),
        ] {
            assert!(value(&field(kind), &v));
        }
    }
}
