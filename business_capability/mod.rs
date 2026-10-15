//! Versioned merchant intent and frozen, executable pure-decision examples in the canonical app package.
use super::*;
use veyndo::component_runtime as wit;
pub(crate) mod snapshot;
#[cfg(test)]
mod tests;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Contract {
    pub version: u8,
    pub intent: HashMap<String, String>,
    #[serde(default)]
    pub assumptions: Vec<HashMap<String, String>>,
    #[serde(default)]
    pub examples: Vec<Example>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Example {
    pub id: String,
    pub label: HashMap<String, String>,
    pub hook: String,
    pub snapshot: snapshot::Frozen,
    pub context: Value,
    pub expected: Expected,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Expected {
    pub adjustment: snapshot::Amount,
    pub accepted: bool,
    pub reason_code: String,
}
fn localized(value: &HashMap<String, String>) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.iter().all(|(key, text)| {
            key.len() <= 35 && !key.is_empty() && !text.trim().is_empty() && text.len() <= 2000
        })
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    let Some(c) = &m.capability else {
        return Ok(());
    };
    if c.version != 1
        || !localized(&c.intent)
        || c.assumptions.len() > 12
        || !c.assumptions.iter().all(localized)
        || c.examples.len() > 12
        || json!(c).to_string().len() > 32768
    {
        return Err(bad(
            "Invalid business capability version, intent or example budget",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for (index, example) in c.examples.iter().enumerate() {
        if !identifier(&example.id)
            || !ids.insert(&example.id)
            || !localized(&example.label)
            || !example.context.is_object()
            || example.context.to_string().len() > 8192
            || !m
                .commerce_hooks
                .as_ref()
                .is_some_and(|hooks| hooks.hooks.contains(&example.hook))
            || !["price", "shipping", "discount", "validation"].contains(&example.hook.as_str())
        {
            return Err(Error::validation(
                &format!("/capability/examples/{index}"),
                "Invalid pure-decision example or undeclared hook".into(),
            ));
        }
        example.snapshot.validate(m)?;
        let result = &example.expected;
        if !result.adjustment.valid()
            || result.adjustment.currency != example.snapshot.subtotal.currency
            || result.adjustment.scale != example.snapshot.subtotal.scale
            || result.adjustment.minor.unsigned_abs() > 100_000_000
            || (example.hook == "validation" && result.adjustment.minor != 0)
            || result.reason_code.len() > 100
            || !result
                .reason_code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(bad("Invalid expected commerce decision"));
        }
    }
    Ok(())
}
/// The real bounded component runtime evaluates each case; there is no model or alternate pricing engine.
pub(super) fn verify(m: &Manifest, guest: &Sandbox) -> Result<()> {
    for (index, example) in m.capability.iter().flat_map(|c| &c.examples).enumerate() {
        let kind = match example.hook.as_str() {
            "price" => wit::Kind::Price,
            "shipping" => wit::Kind::Shipping,
            "discount" => wit::Kind::Discount,
            _ => wit::Kind::Validation,
        };
        let path = format!("/capability/examples/{index}/expected");
        let out = guest
            .evaluate(
                example.snapshot.to_wit(),
                kind,
                &example.context.to_string(),
            )
            .map_err(|_| {
                Error::validation(
                    &path,
                    format!(
                        "Business example {} could not execute within the component contract",
                        example.id
                    ),
                )
            })?;
        let actual = Expected {
            adjustment: snapshot::Amount {
                minor: out.adjustment.minor,
                currency: out.adjustment.currency,
                scale: out.adjustment.scale,
            },
            accepted: out.accepted,
            reason_code: out.reason_code,
        };
        if actual != example.expected {
            return Err(Error::validation(
                &path,
                format!(
                    "Business example {} does not match its expected decision; review the example or component",
                    example.id
                ),
            ));
        }
    }
    Ok(())
}
