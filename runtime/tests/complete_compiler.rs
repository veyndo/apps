//! Full compiler regression against real package fixtures and invalid declarations in every standard domain; no host can skip a stage callback.
#[path = "../examples/support/compiler_host.rs"]
mod host;
use serde_json::{Value, json};
use veyndo_app_model::{
    compiler,
    manifest::{Manifest, canonical_digest},
    package::CompileError,
};
fn changed(id: &str, pointer: &str, value: Value) -> Manifest {
    let mut raw = json!(host::package(id));
    let parts: Vec<_> = pointer.trim_start_matches('/').split('/').collect();
    let mut parent = &mut raw;
    for part in &parts[..parts.len() - 1] {
        parent = if parent.is_array() {
            &mut parent[part.parse::<usize>().unwrap()]
        } else {
            &mut parent[*part]
        };
    }
    let key = parts.last().unwrap();
    if parent.is_array() {
        parent[key.parse::<usize>().unwrap()] = value
    } else {
        parent[*key] = value
    };
    serde_json::from_value(raw).unwrap()
}
#[test]
fn all_34_full_packages_use_one_complete_compiler_and_keep_canonical_bytes() {
    let fixtures = host::fixtures();
    assert_eq!(fixtures.len(), 34);
    for f in fixtures {
        let m: Manifest = serde_json::from_value(f["manifest"].clone()).unwrap();
        compiler::compile(&m, &host::Host::default()).unwrap_or_else(|e| panic!("{}: {e:?}", m.id));
        assert_eq!(canonical_digest(&m), f["digest"].as_str().unwrap());
    }
}
#[test]
fn malformed_metadata_in_all_nineteen_domain_stages_is_rejected() {
    let cases = [
        (
            "object_context",
            "/entities/0/fields/0/unique",
            json!(false),
        ),
        ("production_review", "/lifecycles/0/states", json!([])),
        ("migration", "/settings/0/key", json!("bad.key")),
        (
            "furniture_configurator",
            "/configuration/currency",
            json!("UNKNOWN"),
        ),
        (
            "care_studio",
            "/presentation",
            json!({"icon":"javascript:alert(1)"}),
        ),
        (
            "ontology_care",
            "/intelligence/ontology/0/fields",
            json!(["invented"]),
        ),
        (
            "care_studio",
            "/schemaMigrations",
            json!([{"fromVersion":"bad","steps":[]}]),
        ),
        (
            "care_studio",
            "/distribution",
            json!({"publisher":"bad.id","keyId":"main","signature":"","channel":"stable"}),
        ),
        (
            "snapshot_pricing",
            "/commerceHooks/source",
            json!("(module)"),
        ),
        ("furniture_configurator", "/capability/version", json!(2)),
        (
            "example_payments",
            "/paymentProvider/apiVersion",
            json!("2"),
        ),
        ("care_studio", "/id", json!("bad.id")),
        ("catalog_export", "/events", json!(["*"])),
        ("catalog_export", "/eventDelivery/batchSize", json!(0)),
        (
            "care_studio",
            "/schedules",
            json!([{"id":"tick","cron":"0 * * * * *","action":"unknown","input":{},"enabled":true}]),
        ),
        (
            "care_studio",
            "/webhooks",
            json!([{"id":"incoming","action":"unknown"}]),
        ),
        (
            "care_studio",
            "/surfaces/0/location",
            json!("invalid.location"),
        ),
        ("care_studio", "/intelligence/tools", json!(["invented"])),
        ("care_studio", "/views/0/blocks/0/kind", json!("shell_exec")),
    ];
    for (id, path, value) in cases {
        let m = changed(id, path, value);
        assert!(
            matches!(
                compiler::compile(&m, &host::Host::default()),
                Err(CompileError::Model(_))
            ),
            "{id} {path}"
        );
    }
}
#[test]
fn first_error_and_host_failures_preserve_original_validation_order() {
    let mut bad = changed(
        "example_payments",
        "/settings",
        json!([{"key":"bad.key","kind":"string","required":false,"label":{"en":"Bad"}}]),
    );
    bad.payment_provider.as_mut().unwrap().api_version = "2".into();
    let err = compiler::compile(&bad, &host::Host::default()).unwrap_err();
    let CompileError::Model(e) = err else {
        panic!("model error expected")
    };
    assert!(e.message.to_lowercase().contains("setting"));
    let mut signed = host::package("care_studio");
    signed.id = "fixture_care".into();
    signed.distribution = Some(
        serde_json::from_value(
            json!({"publisher":"fixture","keyId":"main","signature":"","channel":"stable"}),
        )
        .unwrap(),
    );
    signed.category = Some("unknown".into());
    let p = host::Host::default();
    assert!(matches!(
        compiler::compile(&signed, &p),
        Err(CompileError::Host("no registered publisher"))
    ));
    assert_eq!(&*p.calls.borrow(), &["signature"]);
    let mut event = host::package("catalog_export");
    event.event_delivery.as_mut().unwrap().url = Some("https://events.example.test/events".into());
    event.permissions.push("events.send".into());
    let p = host::Host {
        deny: "destination",
        ..Default::default()
    };
    assert!(matches!(
        compiler::compile(&event, &p),
        Err(CompileError::Host("destination"))
    ));
    assert_eq!(&*p.calls.borrow(), &["namespace", "destination"]);
    let p = host::Host::default();
    compiler::compile(&event, &p).unwrap();
    assert_eq!(&*p.calls.borrow(), &["namespace", "destination", "clock"]);
}
#[test]
fn mandatory_native_policy_denials_are_not_replaced_by_metadata() {
    for (id, deny) in [
        ("object_context", "reference"),
        ("furniture_configurator", "currency"),
        ("marketplaces", "scope"),
        ("care_studio", "readonly"),
        ("care_studio", "namespace"),
    ] {
        let p = host::Host {
            deny,
            ..Default::default()
        };
        assert!(
            compiler::compile(&host::package(id), &p).is_err(),
            "{id} {deny}"
        );
    }
    let m = changed("care_studio", "/id", json!("reserved_app"));
    assert!(matches!(
        compiler::compile(&m, &host::Host::default()),
        Err(CompileError::Host("reserved namespace"))
    ));
}
#[test]
fn invalid_surface_metadata_cannot_replace_actual_native_view_lookup() {
    let m = changed(
        "care_studio",
        "/surfaces/0/uiPath",
        json!("native/invented"),
    );
    assert!(compiler::compile(&m, &host::Host::default()).is_err());
    let mut m = host::package("care_studio");
    m.views[0].blocks[0].title.clear();
    assert!(compiler::compile(&m, &host::Host::default()).is_err());
}
