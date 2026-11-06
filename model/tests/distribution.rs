//! Mandatory signature port ordering, metadata validation and bounded installed-dependency graph regressions.
use serde_json::{Value, json};
use std::cell::Cell;
use veyndo_app_model::{Kind, distribution::*, manifest::Manifest};
fn manifest() -> Manifest {
    serde_json::from_value(json!({"id":"fixture_app","version":"1.0.0","coreApi":"1","runtime":"declarative","name":{"en":"Fixture"},"permissions":[],"distribution":{"publisher":"fixture","keyId":"main","channel":"stable","signature":"synthetic"}})).unwrap()
}
struct Verifier {
    calls: Cell<usize>,
    allow: bool,
}
impl SignatureVerifier for Verifier {
    type Error = &'static str;
    fn verify_signature(&self, _: &Manifest) -> Result<(), Self::Error> {
        self.calls.set(self.calls.get() + 1);
        if self.allow {
            Ok(())
        } else {
            Err("revoked operator key")
        }
    }
}
fn dependency(app: &str, version: &str) -> Dependency {
    serde_json::from_value(json!({"app":app,"publisher":"fixture","version":version})).unwrap()
}
#[test]
fn signatures_cannot_skip_current_operator_verification_and_keep_original_error_order() {
    let v = Verifier {
        calls: Cell::new(0),
        allow: false,
    };
    let mut m = manifest();
    assert!(matches!(
        verify(&m, &v),
        Err(VerifyError::Host("revoked operator key"))
    ));
    assert_eq!(v.calls.get(), 1);
    m.distribution.as_mut().unwrap().channel = "invalid".into();
    assert!(matches!(verify(&m, &v), Err(VerifyError::Model(_))));
    assert_eq!(v.calls.get(), 1);
    m.distribution.as_mut().unwrap().channel = "stable".into();
    m.distribution
        .as_mut()
        .unwrap()
        .dependencies
        .push(dependency("fixture_base", "???"));
    assert!(matches!(verify(&m, &v), Err(VerifyError::Host(_))));
    let allowed = Verifier {
        calls: Cell::new(0),
        allow: true,
    };
    assert!(matches!(verify(&m, &allowed), Err(VerifyError::Model(_))));
    assert_eq!(allowed.calls.get(), 1);
    m.distribution = None;
    verify(&m, &v).unwrap();
    assert_eq!(v.calls.get(), 2);
}
#[test]
fn signed_namespace_dependencies_and_count_limits_remain_closed() {
    let v = Verifier {
        calls: Cell::new(0),
        allow: true,
    };
    for (key, value) in [
        ("publisher", json!("bad-id")),
        ("publisher", json!("publisher_too_long")),
        ("keyId", json!("bad.id")),
        ("channel", json!("production")),
        (
            "dependencies",
            json!(vec![
                json!({"app":"fixture_base","publisher":"fixture","version":"^1"});
                13
            ]),
        ),
    ] {
        let mut raw = json!(manifest());
        raw["distribution"][key] = value;
        let m = serde_json::from_value(raw).unwrap();
        assert!(verify(&m, &v).is_err());
    }
    for dep in [
        dependency("fixture_app", "^1"),
        dependency("foreign_app", "^1"),
        dependency("fixture_base", "??"),
    ] {
        let mut m = manifest();
        m.distribution.as_mut().unwrap().dependencies = vec![dep];
        assert!(verify(&m, &v).is_err());
    }
    let mut m = manifest();
    let d = dependency("fixture_base", "^1");
    m.distribution.as_mut().unwrap().dependencies = vec![d.clone(), d];
    assert!(verify(&m, &v).is_err());
}
#[test]
fn required_dependencies_check_signed_publisher_and_installed_semver() {
    let mut prior = manifest();
    prior.id = "fixture_base".into();
    let dep = dependency("fixture_base", "^1.0.0");
    require_version(&dep, &prior, "1.2.0").unwrap();
    assert_eq!(
        require_version(&dep, &prior, "2.0.0").unwrap_err().kind,
        Kind::Conflict
    );
    assert_eq!(
        require_version(&dep, &prior, "invalid").unwrap_err().kind,
        Kind::Bad
    );
    prior.distribution = None;
    assert_eq!(
        require_version(&dep, &prior, "1.0.0").unwrap_err().kind,
        Kind::Conflict
    );
    assert_eq!(
        require_version(&dependency("fixture_base", "???"), &manifest(), "1.0.0")
            .unwrap_err()
            .kind,
        Kind::Bad
    );
}
fn installed(id: &str, deps: Vec<Dependency>) -> Value {
    json!({"id":id,"distribution":{"publisher":"fixture","dependencies":deps}})
}
#[test]
fn cycles_reachable_budget_and_active_dependants_block_incompatible_changes() {
    let mut m = manifest();
    m.distribution
        .as_mut()
        .unwrap()
        .dependencies
        .push(dependency("fixture_base", "^1"));
    let cycle = vec![installed(
        "fixture_base",
        vec![dependency("fixture_app", "^1")],
    )];
    assert_eq!(validate_graph(&m, &cycle).unwrap_err().kind, Kind::Conflict);
    let mut huge = Vec::new();
    for i in 0..257 {
        let next = if i == 256 {
            vec![]
        } else {
            vec![dependency(&format!("fixture_{:03}", i + 1), "^1")]
        };
        huge.push(installed(&format!("fixture_{i:03}"), next));
    }
    m.distribution.as_mut().unwrap().dependencies = vec![dependency("fixture_000", "^1")];
    assert_eq!(validate_graph(&m, &huge).unwrap_err().kind, Kind::Bad);
    m.distribution.as_mut().unwrap().dependencies.clear();
    let active = vec![installed(
        "fixture_child",
        vec![dependency("fixture_app", "^1")],
    )];
    validate_graph(&m, &active).unwrap();
    m.version = "2.0.0".into();
    assert_eq!(
        validate_graph(&m, &active).unwrap_err().kind,
        Kind::Conflict
    );
    m.version = "1.1.0".into();
    m.distribution.as_mut().unwrap().publisher = "other".into();
    assert_eq!(
        validate_graph(&m, &active).unwrap_err().kind,
        Kind::Conflict
    );
}
