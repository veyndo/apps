//! Complete native wire/fingerprint fixtures and strict passive/payment contracts; live authority is not inferred from decoding.
use serde_json::{Value, json};
use veyndo_app_model::{
    manifest::{Manifest, canonical_digest, signature_message},
    payment_provider, presentation,
};
fn fixtures() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/complete_packages.json")).unwrap()
}
fn package(name: &str) -> Manifest {
    let v = fixtures()
        .into_iter()
        .find(|v| v["path"].as_str().unwrap().contains(&format!("/{name}/")))
        .unwrap();
    serde_json::from_value(v["manifest"].clone()).unwrap()
}
#[test]
fn complete_decoder_preserves_all_34_native_canonical_digests() {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), 34);
    for fixture in fixtures {
        let m: Manifest = serde_json::from_value(fixture["manifest"].clone()).unwrap();
        assert_eq!(
            canonical_digest(&m),
            fixture["digest"].as_str().unwrap(),
            "{}",
            fixture["path"]
        );
        assert_eq!(m.common_contract().id, m.id);
        assert_eq!(
            m.common_contract().has_payment_provider,
            m.payment_provider.is_some()
        );
    }
}
#[test]
fn full_package_unknown_fields_cannot_hide_new_authority() {
    for fixture in fixtures() {
        let mut raw = fixture["manifest"].clone();
        raw["runtimeAuthority"] = json!({"admin":true});
        assert!(serde_json::from_value::<Manifest>(raw).is_err());
    }
    let mut raw = json!(package("payment-provider"));
    raw["paymentProvider"]["methods"][0]["secret"] = json!("not-accepted");
    assert!(serde_json::from_value::<Manifest>(raw).is_err());
    let mut raw = json!(package("care-studio"));
    raw["distribution"] = json!({"publisher":"fixture","keyId":"main","signature":"","channel":"stable","trust":true});
    assert!(serde_json::from_value::<Manifest>(raw).is_err());
}
#[test]
fn optional_metadata_and_legacy_defaults_keep_their_exact_wire() {
    let m = package("care-studio");
    let v = json!(m);
    assert!(v.get("presentation").is_none());
    assert!(v.get("distribution").is_none());
    assert!(signature_message(&m).is_none());
    let mut raw = v.clone();
    raw["presentation"] = json!({"icon":null,"cover":null,"description":{}});
    let parsed: Manifest = serde_json::from_value(raw).unwrap();
    assert_eq!(json!(parsed)["presentation"], json!({}));
    let minimal:Manifest=serde_json::from_value(json!({"id":"minimal","version":"1.0.0","coreApi":"1","runtime":"declarative","name":{"en":"Minimal"},"permissions":[]})).unwrap();
    let wire = json!(minimal);
    for key in ["entities", "slots", "actions", "events"] {
        assert_eq!(wire[key], json!([]));
    }
    assert!(wire.get("views").is_none());
}
#[test]
fn signing_message_covers_full_package_except_its_own_signature() {
    let mut m = package("care-studio");
    m.id = "fixture_care".into();
    m.distribution = Some(
        serde_json::from_value(
            json!({"publisher":"fixture","keyId":"main","channel":"stable","signature":"one"}),
        )
        .unwrap(),
    );
    let message = signature_message(&m).unwrap();
    assert!(
        String::from_utf8(message.clone())
            .unwrap()
            .starts_with("veyndo-package-v1\n")
    );
    m.distribution.as_mut().unwrap().signature = "two".into();
    assert_eq!(signature_message(&m).unwrap(), message);
    for mutate in [0, 1, 2, 3, 4] {
        let mut changed = m.clone();
        match mutate {
            0 => changed.permissions.push("customers.pii".into()),
            1 => changed.distribution.as_mut().unwrap().channel = "beta".into(),
            2 => changed.version = "2.0.0".into(),
            3 => {
                changed.name.insert("de-DE".into(), "Geändert".into());
            }
            _ => changed.distribution.as_mut().unwrap().dependencies.push(
                serde_json::from_value(
                    json!({"app":"fixture_base","publisher":"fixture","version":"^1"}),
                )
                .unwrap(),
            ),
        };
        assert_ne!(signature_message(&changed).unwrap(), message);
    }
}
#[test]
fn passive_artwork_retains_exact_https_and_local_path_grammar() {
    for value in [
        "/media/shop/cover.webp",
        "/assets/app.svg",
        "https://example.test/app.png",
    ] {
        let p =
            serde_json::from_value(json!({"icon":value,"cover":value,"description":{"es":"Guía"}}))
                .unwrap();
        presentation::validate(Some(&p)).unwrap();
    }
    for value in [
        "javascript:alert(1)",
        "data:image/svg+xml,x",
        "//example.test/x",
        "http://example.test/x",
        "https://user:secret@example.test/x",
        "/media/../secret",
        "file:///tmp/a",
    ] {
        let p = serde_json::from_value(json!({"icon":value})).unwrap();
        assert!(presentation::validate(Some(&p)).is_err(), "{value}");
    }
    for desc in [
        json!({"":"x"}),
        json!({"bad_locale":"x"}),
        json!({"en":"x".repeat(1201)}),
    ] {
        let p = serde_json::from_value(json!({"description":desc})).unwrap();
        assert!(presentation::validate(Some(&p)).is_err());
    }
}
#[test]
fn payment_declarations_keep_native_ids_runtime_consent_and_capability_limits() {
    let m = package("payment-provider");
    payment_provider::validate_contract(&m).unwrap();
    for id in ["paypal", "manual", "simulated"] {
        let mut bad = m.clone();
        bad.id = id.into();
        assert!(payment_provider::validate_contract(&bad).is_err());
    }
    for raw in [
        json!({"runtime":"declarative"}),
        json!({"category":"api"}),
        json!({"permissions":[]}),
    ] {
        let mut v = json!(m);
        for (k, val) in raw.as_object().unwrap() {
            v[k] = val.clone();
        }
        assert!(payment_provider::validate_contract(&serde_json::from_value(v).unwrap()).is_err());
    }
    for (field, value) in [
        ("currencies", json!(["eur"])),
        ("countries", json!(["DEU"])),
        ("checkout", json!("script")),
        ("intent", json!("refund")),
        ("capabilities", json!(["refund"])),
        ("id", json!("sql;exec")),
    ] {
        let mut v = json!(m);
        v["paymentProvider"]["methods"][0][field] = value;
        assert!(payment_provider::validate_contract(&serde_json::from_value(v).unwrap()).is_err());
    }
    let mut duplicate = m.clone();
    let method = duplicate.payment_provider.as_ref().unwrap().methods[0].clone();
    duplicate
        .payment_provider
        .as_mut()
        .unwrap()
        .methods
        .push(method);
    assert!(payment_provider::validate_contract(&duplicate).is_err());
}
#[test]
fn multiple_capture_keeps_required_operational_dependencies() {
    let mut m = package("payment-provider");
    m.payment_provider.as_mut().unwrap().methods[0]
        .capabilities
        .push("multi_capture".into());
    payment_provider::validate_contract(&m).unwrap();
    for required in ["capture", "authorize", "void"] {
        let mut bad = m.clone();
        bad.payment_provider.as_mut().unwrap().methods[0]
            .capabilities
            .retain(|c| c != required);
        assert!(payment_provider::validate_contract(&bad).is_err());
    }
}

#[test]
fn downstream_json_order_cannot_change_nested_package_fingerprints() {
    use veyndo_app_model::manifest::value_digest;
    let first: Value =
        serde_json::from_str(r#"{"z":{"b":1.5,"a":-0.0},"a":[{"y":2,"x":3}]}"#).unwrap();
    let second: Value =
        serde_json::from_str(r#"{"a":[{"x":3,"y":2}],"z":{"a":-0.0,"b":1.5}}"#).unwrap();
    assert_eq!(value_digest(&first), value_digest(&second));
    assert_ne!(
        value_digest(&first),
        value_digest(&json!({"a":[{"x":3,"y":2}],"z":{"a":0,"b":1.5}}))
    );
}
