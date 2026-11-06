//! Versioned localized intent and frozen commerce examples; these declarations do not authorize live data or effects.
use crate::{Entity, Error, Result, bad, commerce_hooks, declarations::identifier, fields};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
pub struct Context<'a> {
    pub entities: &'a [Entity],
    pub commerce_hooks: Option<&'a commerce_hooks::Contract>,
    pub capability: Option<&'a Contract>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contract {
    pub version: u8,
    pub intent: HashMap<String, String>,
    #[serde(default)]
    pub assumptions: Vec<HashMap<String, String>>,
    #[serde(default)]
    pub examples: Vec<Example>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Example {
    pub id: String,
    pub label: HashMap<String, String>,
    pub hook: String,
    pub snapshot: crate::business_snapshot::Frozen,
    pub context: Value,
    pub expected: Expected,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expected {
    pub adjustment: crate::business_snapshot::Amount,
    pub accepted: bool,
    pub reason_code: String,
}
pub fn localized(value: &HashMap<String, String>) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.iter().all(|(key, text)| {
            key.len() <= 35 && !key.is_empty() && !text.trim().is_empty() && text.len() <= 2000
        })
}
pub fn validate<P: fields::ValuePolicy>(m: &Context<'_>, policy: &P) -> Result<()> {
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
                "Invalid pure-decision example or undeclared hook",
            ));
        }
        example.snapshot.validate(m, policy)?;
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
