//! Exact app-field predicates validated against current packages and applied before native pagination.
use super::{Manifest, core_extensions::Object, editor_contract};
use crate::{App, RequestContext, Result, bad, merchant};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filter {
    pub app: String,
    pub entity: String,
    pub field: String,
    pub value: Value,
    #[serde(skip)]
    anchor: String,
    #[serde(skip)]
    digest: String,
}

pub(crate) fn decode(input: &str) -> Result<Vec<Filter>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if input.len() > 4096 {
        return Err(bad("App filter exceeds 4096 bytes"));
    }
    let filters: Vec<Filter> =
        serde_json::from_str(input).map_err(|_| bad("Invalid app filter"))?;
    if filters.is_empty()
        || filters.len() > 4
        || filters.iter().any(|f| {
            [&f.app, &f.entity, &f.field]
                .iter()
                .any(|s| !super::identifier(s))
                || !(f.value.is_string() || f.value.is_i64() || f.value.is_boolean())
                || f.value.as_str().is_some_and(|s| s.len() > 2000)
        })
    {
        return Err(bad("Use 1..4 exact indexed app-field filters"));
    }
    Ok(filters)
}

pub(crate) async fn admit(
    a: &App,
    h: &RequestContext,
    object: Object,
    input: &str,
) -> Result<Vec<Filter>> {
    let mut filters = decode(input)?;
    if filters.is_empty() {
        return Ok(filters);
    }
    let t = merchant(a, h)?;
    for f in &mut filters {
        let m: Manifest = super::package(a, &t, &f.app, true).await?;
        let e = m
            .entities
            .iter()
            .find(|e| e.name == f.entity)
            .ok_or(bad("Unknown extension model"))?;
        let x = e
            .extends
            .as_ref()
            .filter(|x| x.object == object)
            .ok_or(bad("App model does not extend this native object"))?;
        editor_contract::entity_access(&m, e, h, "list")?;
        let field = e
            .fields
            .iter()
            .find(|field| field.name == f.field && field.indexed && !field.translatable)
            .ok_or(bad(
                "Only declared indexed non-translated app fields are searchable",
            ))?;
        if !match field.kind.as_str() {
            "string" => f.value.is_string(),
            "integer" => f.value.is_i64(),
            "boolean" => f.value.is_boolean(),
            _ => false,
        } {
            return Err(bad("App filter type does not match the installed model"));
        }
        f.anchor = x.field.clone();
        f.digest = crate::hash(&serde_json::json!(m).to_string());
    }
    Ok(filters)
}

pub(crate) fn predicate(
    q: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    object: Object,
    t: &str,
    filters: &[Filter],
) {
    let (column, id) = match object {
        Object::Product => ("product_id", "p.id"),
        Object::Order => ("order_id", "o.id"),
        Object::Customer => ("customer_id", "contact->>'id'"),
        Object::Company => ("company_id", "id"),
    };
    for f in filters {
        q.push(" AND EXISTS(SELECT 1 FROM app_record_references xr JOIN app_record_indexes xi ON xi.tenant=xr.tenant AND xi.app=xr.app AND xi.entity=xr.entity AND xi.record_id=xr.record_id JOIN app_packages xp ON xp.tenant=xr.tenant AND xp.id=xr.app AND xp.active WHERE xr.tenant=").push_bind(t.to_owned())
            .push(" AND xp.digest=").push_bind(f.digest.clone())
            .push(" AND xr.app=").push_bind(f.app.clone()).push(" AND xr.entity=").push_bind(f.entity.clone())
            .push(" AND xr.field=").push_bind(f.anchor.clone()).push(" AND xr.").push(column).push("=").push(id)
            .push(" AND xi.field=").push_bind(f.field.clone())
            .push(" AND xi.value_hash=sha256(jsonb_send(").push_bind(f.value.clone()).push("::jsonb)) AND xi.value=").push_bind(f.value.clone()).push(")");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_identity_injection_or_unbounded_or_untyped_filter() {
        for value in [
            "{}",
            "[]",
            r#"[{"app":"a","entity":"b","field":"x","value":null}]"#,
            r#"[{"app":"a","entity":"b","field":"x","value":[],"tenant":"other"}]"#,
            r#"[{"app":"a;DROP","entity":"b","field":"x","value":"s"}]"#,
        ] {
            assert!(decode(value).is_err());
        }
        assert_eq!(
            decode(r#"[{"app":"a","entity":"b","field":"x","value":false}]"#)
                .unwrap()
                .len(),
            1
        );
    }
}
