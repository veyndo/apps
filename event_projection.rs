//! Least-privilege event subscriptions and payload projections; merchant identity never implies app access.
use super::*;
fn permission(m: &Manifest, value: &str) -> bool {
    m.permissions.iter().any(|p| p == value)
}
pub(super) fn eligible(m: &Manifest, kind: &str, data: &Value) -> bool {
    subscribed(m, kind)
        && (kind != "app.record.changed" || data["app"] == m.id)
        && event_contract::filtered(m, kind, data)
}
pub(super) fn subscribed(m: &Manifest, kind: &str) -> bool {
    let declared = m.events.iter().any(|event| {
        event == kind
            || event
                .strip_suffix(".*")
                .is_some_and(|prefix| kind.starts_with(&format!("{prefix}.")))
    });
    let family = kind.split('.').next().unwrap_or("");
    let scoped =
        permission(m, &format!("events:{kind}")) || permission(m, &format!("events:{family}.*"));
    let own = kind.starts_with(&format!("app.{}.", m.id)) && permission(m, "events:self");
    // Legacy permission retains minimized order/consumer notifications only. Never team/customer/app secrets.
    let legacy = permission(m, "events.read") && ["order", "consumer"].contains(&family);
    declared && (scoped || own || legacy)
}
pub(super) fn payload(m: &Manifest, kind: &str, data: Value) -> Value {
    if permission(m, "customers.pii")
        && (kind.starts_with("customer.")
            || kind.starts_with("consumer.")
            || kind.starts_with("order."))
    {
        return data;
    }
    let mut clean = serde_json::Map::new();
    if let Some(obj) = data.as_object() {
        let keys: &[&str] = if kind.starts_with(&format!("app.{}.", m.id)) {
            return data;
        } else if kind.starts_with("order.") {
            &[
                "orderId",
                "number",
                "totalMinor",
                "currency",
                "currencyScale",
                "state",
                "paymentState",
                "deliveryState",
                "simulation",
                "salesChannelId",
            ]
        } else if kind.starts_with("customer.") {
            &["customerId", "id", "revision"]
        } else if kind.starts_with("product.") {
            &["productId", "id", "revision", "stock"]
        } else if kind.starts_with("consumer.") {
            &["requestId", "id", "customerId", "orderId", "kind", "locale"]
        } else if kind.starts_with("intelligence.") || kind == "merchant.change.applied" {
            &[
                "id",
                "claimId",
                "taskId",
                "revision",
                "state",
                "operation",
                "channelId",
                "causalUpliftProven",
                "asOf",
                "autonomous",
            ]
        } else if kind == "app.record.changed" {
            &["app", "entity", "id", "revision"]
        } else {
            &["id", "revision", "kind"]
        };
        for key in keys {
            if let Some(value) = obj.get(*key) {
                clean.insert((*key).into(), value.clone());
            }
        }
    }
    Value::Object(clean)
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    if m.events
        .iter()
        .any(|e| !subscribed(m, e.trim_end_matches(".*")))
    {
        // Check wildcard declarations against a concrete member of the family.
        for e in &m.events {
            let concrete = if let Some(prefix) = e.strip_suffix(".*") {
                format!("{prefix}.sample")
            } else {
                e.clone()
            };
            if !subscribed(m, &concrete) {
                return Err(bad(
                    "Event subscription requires a matching events: permission",
                ));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intelligence_requires_scoped_subscription_and_projects_no_private_evidence() {
        let mut m: Manifest = serde_json::from_str(approval::built_in("slack").unwrap()).unwrap();
        m.events = vec!["intelligence.experiment.result".into()];
        m.permissions = vec!["events.read".into(), "knowledge.read".into()];
        assert!(!subscribed(&m, "intelligence.experiment.result"));
        m.permissions.push("events:intelligence.*".into());
        assert!(subscribed(&m, "intelligence.experiment.result"));
        let data = json!({"id":"experiment", "channelId":"default", "causalUpliftProven":false,
            "actor":"private-user", "email":"private@example.test", "evidence":{"quote":"private support mail"}});
        assert_eq!(
            payload(&m, "intelligence.experiment.result", data),
            json!({"id":"experiment", "channelId":"default", "causalUpliftProven":false})
        );
        m.permissions.push("customers.pii".into());
        assert_eq!(
            payload(
                &m,
                "intelligence.claim.reviewed",
                json!({"claimId":"claim","state":"confirmed","actor":"private-user"})
            ),
            json!({"claimId":"claim","state":"confirmed"})
        );
    }
    #[test]
    fn coarse_events_do_not_disclose_customers_team_or_other_apps() {
        let mut m: Manifest = serde_json::from_str(approval::built_in("slack").unwrap()).unwrap();
        m.permissions = vec!["events.read".into()];
        for kind in [
            "customer.address_changed",
            "membership.updated",
            "app.other.changed",
        ] {
            m.events.push(kind.into());
            assert!(!subscribed(&m, kind));
        }
        let p = payload(
            &m,
            "order.placed",
            json!({"orderId":"own","totalMinor":100,"email":"private@example.test","address":{"street":"private"}}),
        );
        assert_eq!(p, json!({"orderId":"own","totalMinor":100}));
        m.permissions.push("customers.pii".into());
        assert_eq!(
            payload(
                &m,
                "order.placed",
                json!({"email":"consented@example.test"})
            ),
            json!({"email":"consented@example.test"})
        );
    }
}
