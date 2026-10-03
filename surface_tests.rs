//! Contract counterexamples reject cross-scope UI actions, unsafe URLs and mutating GET routes.
use super::*;
#[test]
fn the_full_app_example_has_valid_surfaces_routes_and_intelligence() {
    let m: Manifest = serde_json::from_str(include_str!(
        "../../extensions/apps/product-lab/manifest.json"
    ))
    .unwrap();
    validate(&m).unwrap();
    for path in [
        "../escape",
        "https://example.test/x",
        "//foreign",
        "ui?token=x",
        "ui#x",
        "%2e%2e/ui",
    ] {
        let mut broken = m.clone();
        broken.surfaces[0].ui_path = path.into();
        assert!(validate(&broken).is_err());
    }
    let mut broken = m.clone();
    broken.surfaces[1].actions.push("save_entry".into());
    assert!(validate(&broken).is_err());
    let mut broken = m.clone();
    broken.api_routes[0].method = "GET".into();
    broken.api_routes[0].action = "support_received".into();
    assert!(validate(&broken).is_err());
    let mut broken = m.clone();
    broken
        .intelligence
        .as_mut()
        .unwrap()
        .entities
        .push("unknown".into());
    assert!(validate(&broken).is_err());
}
