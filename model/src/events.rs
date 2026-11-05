//! Strict event filter/delivery declarations with a mandatory trusted destination-policy port; no network or queue owner.
use crate::{bad, declarations::identifier, package::CompileError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
/// Construct only from the host's fully validated and consented current package.
pub struct Context<'a> {
    pub id: &'a str,
    pub events: &'a [String],
    pub permissions: &'a [String],
    pub event_filters: &'a [EventFilter],
}
/// The host must validate URL syntax and its network policy; there is no default acceptor.
pub trait DestinationPolicy {
    type Error;
    fn validate_destination(&self, url: &str) -> std::result::Result<(), Self::Error>;
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventFilter {
    pub event: String,
    pub equals: HashMap<String, Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventDelivery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default = "single")]
    pub batch_size: u16,
}
fn single() -> u16 {
    1
}
pub fn matches(pattern: &str, event: &str) -> bool {
    pattern == event
        || pattern
            .strip_suffix(".*")
            .is_some_and(|p| event.starts_with(&format!("{p}.")))
}
pub fn filtered(m: &Context<'_>, kind: &str, data: &Value) -> bool {
    let filters = m
        .event_filters
        .iter()
        .filter(|f| matches(&f.event, kind))
        .collect::<Vec<_>>();
    if filters.is_empty() {
        return true;
    }
    let projected = crate::subscriptions::payload(m, kind, data.clone());
    filters
        .iter()
        .any(|f| f.equals.iter().all(|(k, v)| projected.get(k) == Some(v)))
}
pub fn validate<P: DestinationPolicy>(
    m: &Context<'_>,
    delivery: Option<&EventDelivery>,
    policy: &P,
) -> std::result::Result<(), CompileError<P::Error>> {
    if m.event_filters.len() > 24 {
        return Err(CompileError::Model(bad("Maximum 24 event filters")));
    }
    for f in m.event_filters {
        if !m.events.contains(&f.event)
            || f.equals.is_empty()
            || f.equals.len() > 4
            || f.equals.iter().any(|(k, v)| {
                !identifier(k)
                    || !matches!(
                        v,
                        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
                    )
                    || v.to_string().len() > 1000
            })
        {
            return Err(CompileError::Model(bad(
                "Filters require a declared event and 1..4 bounded scalar equalities",
            )));
        }
    }
    if let Some(d) = delivery {
        if m.events.is_empty() || !(1..=25).contains(&d.batch_size) {
            return Err(CompileError::Model(bad(
                "Event delivery needs subscriptions and batchSize 1..25",
            )));
        }
        if let Some(url) = &d.url {
            policy
                .validate_destination(url)
                .map_err(CompileError::Host)?;
            if !m.permissions.contains(&"events.send".into()) {
                return Err(CompileError::Model(bad(
                    "Outgoing webhooks require events.send permission",
                )));
            }
        }
    }
    Ok(())
}
