//! Versioned app-owned payment methods; declarations never grant financial authority.
use crate::{Result, bad, declarations::identifier, manifest::Manifest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderContract {
    pub api_version: String,
    pub methods: Vec<ProviderMethod>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderMethod {
    pub id: String,
    pub name: HashMap<String, String>,
    pub currencies: Vec<String>,
    pub countries: Vec<String>,
    pub capabilities: Vec<String>,
    pub checkout: String,
    pub intent: String,
}
pub fn validate_contract(m: &Manifest) -> Result<()> {
    let Some(p) = &m.payment_provider else {
        return Ok(());
    };
    if ["paypal", "manual", "simulated"].contains(&m.id.as_str())
        || m.runtime != "service"
        || m.category.as_deref() != Some("payment")
        || !m.permissions.iter().any(|v| v == "payments.provider")
        || p.api_version != "1"
        || p.methods.is_empty()
        || p.methods.len() > 32
    {
        return Err(bad(
            "Payment provider requires service runtime, payment category and API 1",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for method in &p.methods {
        if !identifier(&method.id)
            || !ids.insert(&method.id)
            || method.name.is_empty()
            || method.name.values().any(|n| n.is_empty() || n.len() > 200)
            || method.currencies.is_empty()
            || method.currencies.len() > 32
            || method
                .currencies
                .iter()
                .any(|c| c.len() != 3 || !c.bytes().all(|b| b.is_ascii_uppercase()))
            || method.countries.len() > 300
            || method
                .countries
                .iter()
                .any(|c| c.len() != 2 || !c.bytes().all(|b| b.is_ascii_uppercase()))
            || !["redirect", "embedded"].contains(&method.checkout.as_str())
            || !["capture", "authorize"].contains(&method.intent.as_str())
            || !method.capabilities.iter().any(|c| c == &method.intent)
            || (method.capabilities.iter().any(|c| c == "multi_capture")
                && ["capture", "authorize", "void"]
                    .iter()
                    .any(|required| !method.capabilities.iter().any(|c| c == required)))
            || method.capabilities.len() > 8
            || method.capabilities.iter().any(|c| {
                ![
                    "capture",
                    "authorize",
                    "void",
                    "refund",
                    "recurring",
                    "vault",
                    "multi_capture",
                    "details",
                    "instructions",
                ]
                .contains(&c.as_str())
            })
        {
            return Err(bad("Invalid payment method contract"));
        }
    }
    Ok(())
}
