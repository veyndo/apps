//! Operator service authority is bound to immutable package content, never to a merchant-chosen ID.
use super::*;
pub(super) fn built_in(id: &str) -> Option<&'static str> {
    match id {
        "engraving" => Some(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        )),
        "storyfront" => Some(include_str!(
            "../../extensions/apps/storyfront/manifest.json"
        )),
        "paypal" => Some(include_str!("../../extensions/apps/paypal/manifest.json")),
        "shopware_payments" => Some(include_str!(
            "../../extensions/apps/shopware-payments/manifest.json"
        )),
        "google_analytics" => Some(include_str!(
            "../../extensions/apps/google-analytics/manifest.json"
        )),
        "gmail" => Some(include_str!("../../extensions/apps/gmail/manifest.json")),
        "email" => Some(include_str!("../../extensions/apps/email/manifest.json")),
        "slack" => Some(include_str!("../../extensions/apps/slack/manifest.json")),
        _ => None,
    }
}
pub(super) fn legacy_built_in(id: &str) -> Option<&'static str> {
    match id {
        "engraving" => Some(include_str!("../../reference/app-manifests/engraving.json")),
        "storyfront" => Some(include_str!(
            "../../reference/app-manifests/storyfront.json"
        )),
        "paypal" => Some(include_str!("../../reference/app-manifests/paypal.json")),
        "shopware_payments" => Some(include_str!(
            "../../reference/app-manifests/shopware-payments.json"
        )),
        "google_analytics" => Some(include_str!(
            "../../reference/app-manifests/google-analytics.json"
        )),
        "gmail" => Some(include_str!("../../reference/app-manifests/gmail.json")),
        "email" => Some(include_str!("../../reference/app-manifests/email.json")),
        "slack" => Some(include_str!("../../reference/app-manifests/slack.json")),
        _ => None,
    }
}
pub(super) fn canonical_digest(m: &Manifest) -> String {
    hash(&json!(m).to_string())
}
pub(super) fn approved(m: &Manifest, service: &Value) -> bool {
    let digest = canonical_digest(m);
    let pinned = service["approvedDigests"]
        .as_array()
        .is_some_and(|pins| pins.iter().any(|p| p.as_str() == Some(digest.as_str())));
    let bundled = [built_in(&m.id), legacy_built_in(&m.id)]
        .into_iter()
        .flatten()
        .filter_map(|s| serde_json::from_str::<Manifest>(s).ok())
        .any(|original| canonical_digest(&original) == digest);
    crate::verified_kernel::app_package_authorized(pinned, bundled)
}
pub(super) fn installation(m: &Manifest) -> Result<()> {
    let service = &crate::runtime_config::get().services[&m.id];
    if (built_in(&m.id).is_some() || !service.is_null()) && !approved(m, service) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Package digest is not approved for this service or reserved app ID".into(),
        ));
    }
    Ok(())
}

pub(crate) fn print_digest() {
    let path = env::args()
        .nth(2)
        .expect("Usage: vendune --app-digest MANIFEST.json");
    let source = std::fs::read_to_string(path).expect("Read app manifest");
    let m: Manifest = serde_json::from_str(&source).expect("Typed app manifest");
    println!("{}", canonical_digest(&m));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modified_reserved_payment_package_does_not_inherit_operator_authority() {
        let mut m: Manifest = serde_json::from_str(built_in("shopware_payments").unwrap()).unwrap();
        assert!(approved(&m, &json!({})));
        m.actions.clear();
        m.name.insert("en".into(), "Unapproved replacement".into());
        assert!(!approved(&m, &json!({})));
        assert!(approved(
            &m,
            &json!({"approvedDigests":[canonical_digest(&m)]})
        ));
        m.version = "9.0.0".into();
        assert!(!approved(
            &m,
            &json!({"approvedDigests":["not-the-current-digest"]})
        ));
    }
}
