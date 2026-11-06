//! Original job history redaction and queued-event selection; caller supplies current authority and the actual installed package.
use crate::model::{
    Action,
    admission::{self, ScopeAuthority},
    manifest::Manifest,
};
use serde_json::{Value, json};
pub fn rights<A: ScopeAuthority>(host: &A, action: &Action) -> Result<(), A::Error> {
    host.require_scope(action.permission.as_deref().unwrap_or("apps.manage"))?;
    admission::additional(host, action)
}
pub fn history<A: ScopeAuthority>(host: &A, m: &Manifest, rows: &mut [Value]) {
    let digest = crate::model::manifest::canonical_digest(m);
    for row in rows {
        let allowed = row["packageDigest"] == digest
            && m.actions
                .iter()
                .find(|a| Some(a.name.as_str()) == row["action"].as_str())
                .is_some_and(|a| rights(host, a).is_ok());
        if !allowed {
            row["message"] = json!({});
            row["result"] = Value::Null;
        }
        if let Some(row) = row.as_object_mut() {
            row.remove("packageDigest");
        }
    }
}
pub fn event_kind(m: &Manifest, action: &str) -> String {
    let grouped = format!("app.{}.job_queued", m.id);
    if m.events.contains(&grouped) {
        grouped
    } else {
        format!("app.{}.job_{action}", m.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Rights(Vec<&'static str>);
    impl ScopeAuthority for Rights {
        type Error = ();
        fn require_scope(&self, s: &str) -> Result<(), ()> {
            self.0.contains(&s).then_some(()).ok_or(())
        }
    }
    #[test]
    fn archived_or_revoked_results_are_never_projected() {
        let mut m: Manifest =
            serde_json::from_str(include_str!("../data-service/tests/portable-package.json"))
                .unwrap();
        m.actions[0].required_permissions = vec!["b2b.read".into()];
        let digest = crate::model::manifest::canonical_digest(&m);
        let original = json!({"action":"list_notes","packageDigest":digest,"message":{"de":"Privater Bericht"},"result":{"private":"retained"}});
        for (rights, stale) in [
            (vec!["catalog.read"], false),
            (vec!["b2b.read"], false),
            (vec!["catalog.read", "b2b.read"], true),
        ] {
            let mut row = original.clone();
            if stale {
                row["packageDigest"] = json!("old");
            }
            history(&Rights(rights), &m, std::slice::from_mut(&mut row));
            assert_eq!(row["message"], json!({}));
            assert!(row["result"].is_null());
            assert!(row.get("packageDigest").is_none());
        }
        let mut current = original;
        history(
            &Rights(vec!["catalog.read", "b2b.read"]),
            &m,
            std::slice::from_mut(&mut current),
        );
        assert_eq!(current["result"]["private"], "retained");
    }
}
