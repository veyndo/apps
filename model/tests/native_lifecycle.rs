//! Portable graph admission, frozen snapshots and current-right narrowing; native effects are exercised by the composed host tests.
use serde_json::{Value, json};
use veyndo_app_model::{Kind, native_lifecycle::*, native_lifecycle_snapshot as accepted};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/native_lifecycle.json")).unwrap()
}
struct Host {
    base: Machine,
    scopes: Vec<&'static str>,
    locales: bool,
}
impl CompilerPolicy for Host {
    fn order_base(&self) -> Machine {
        self.base.clone()
    }
    fn scopes(&self) -> &[&str] {
        &self.scopes
    }
    fn locale(&self, s: &str) -> bool {
        self.locales && ["en-GB", "de-DE", "fr-FR", "es-ES", "it", "it-IT"].contains(&s)
    }
}
fn host() -> Host {
    Host {
        base: serde_json::from_value(fixture()["orderBase"].clone()).unwrap(),
        scopes: vec!["documents.read"],
        locales: true,
    }
}
fn contracts(v: &Value) -> Vec<Contract> {
    serde_json::from_value(v["lifecycles"].clone()).unwrap()
}
fn package<'a>(v: &'a Value, c: &'a [Contract], p: &'a [String]) -> Package<'a> {
    Package {
        id: v["id"].as_str().unwrap(),
        version: v["version"].as_str().unwrap(),
        digest: v["digest"].as_str().unwrap(),
        permissions: p,
        lifecycles: c,
    }
}
fn check(v: &Value) -> veyndo_app_model::Result<()> {
    validate(
        &package(v, &contracts(v), &["commerce.lifecycle".into()]),
        &host(),
    )
}
#[test]
fn compiled_graph_namespaces_and_freezes_complete_package_metadata() {
    let v = fixture();
    let c = contracts(&v);
    let p = vec!["commerce.lifecycle".into()];
    let app = package(&v, &c, &p);
    validate(&app, &host()).unwrap();
    let graph = compile(host().order_base(), &[app], "order", &host()).unwrap();
    assert_eq!(graph.states.len(), 5);
    assert_eq!(graph.transitions.len(), 6);
    assert!(!graph.transitions.iter().any(|e| e.id == "complete"));
    let e = graph
        .transitions
        .iter()
        .find(|e| e.id.ends_with("_approve"))
        .unwrap();
    use sha2::Digest;
    let digest = format!("{:x}", sha2::Sha256::digest(b"production_review"));
    assert_eq!(e.id, format!("a{}_approve", &digest[..8]));
    let policy = e.policy.as_ref().unwrap();
    assert_eq!(policy.app, "production_review");
    assert_eq!(policy.version, "1.0.0");
    assert_eq!(policy.digest, "admitted-complete-package-digest");
    assert_eq!(policy.required_permissions, ["documents.read"]);
    assert_eq!(policy.guards, ["payment.ready", "delivery.delivered"]);
    let guide = generation_context(&host());
    assert_eq!(guide["order"]["machine"], json!(host().order_base()));
    assert_eq!(guide["requiredPermissions"], json!(["documents.read"]));
    assert!(
        guide["quote"]["replaceableActions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn consent_bounds_and_declared_objects_are_required() {
    let v = fixture();
    let c = contracts(&v);
    assert!(validate(&package(&v, &c, &[]), &host()).is_err());
    for (pointer, value) in [
        ("/lifecycles/0/version", json!(2)),
        ("/lifecycles/0/object", json!("payment")),
        ("/lifecycles/1/object", json!("order")),
        ("/lifecycles/0/states", json!([])),
        ("/lifecycles/0/transitions", json!([])),
        ("/lifecycles/0/states/0/id", json!("unsafe-id")),
        ("/lifecycles/0/states/0/id", json!("placed")),
        ("/lifecycles/0/transitions/0/to", json!("unknown")),
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(check(&bad).is_err(), "{pointer}");
    }
    let mut too_many = v.clone();
    too_many["lifecycles"]
        .as_array_mut()
        .unwrap()
        .push(v["lifecycles"][0].clone());
    assert!(check(&too_many).is_err());
    for (key, count) in [("states", 9), ("transitions", 25)] {
        let mut bad = v.clone();
        bad["lifecycles"][0][key] = json!(vec![v["lifecycles"][0][key][0].clone(); count]);
        assert!(check(&bad).is_err());
    }
}
#[test]
fn native_financial_and_buyer_decisions_cannot_be_replaced() {
    let v = fixture();
    for target in ["offered", "accepted", "ordered"] {
        let mut bad = v.clone();
        bad["lifecycles"][1]["transitions"][1]["to"] = json!(target);
        assert!(check(&bad).is_err());
    }
    for action in ["cancel_new", "payments.capture", "unknown"] {
        let mut bad = v.clone();
        bad["lifecycles"][0]["transitions"][0]["replaces"] = json!(action);
        assert_eq!(check(&bad).unwrap_err().kind, Kind::Conflict);
    }
    for from in ["completed", "cancelled"] {
        let mut bad = v.clone();
        bad["lifecycles"][0]["transitions"][1]["from"] = json!(from);
        assert!(check(&bad).is_err());
    }
    for target in ["expired", "payment_review", "manual_archive"] {
        let mut h = host();
        h.base.states.push(MachineState {
            id: target.into(),
            label: Default::default(),
            terminal: true,
        });
        let mut bad = v.clone();
        bad["lifecycles"][0]["transitions"][1]["to"] = json!(target);
        assert!(
            validate(
                &package(&bad, &contracts(&bad), &["commerce.lifecycle".into()]),
                &h
            )
            .is_err()
        );
    }
}
#[test]
fn competing_stranded_duplicate_and_executable_graphs_are_rejected() {
    let v = fixture();
    let c = contracts(&v);
    let p = vec!["commerce.lifecycle".into()];
    let mut other = v.clone();
    other["id"] = json!("competitor");
    assert_eq!(
        compile(
            host().order_base(),
            &[package(&v, &c, &p), package(&other, &c, &p)],
            "order",
            &host()
        )
        .err()
        .unwrap()
        .kind,
        Kind::Conflict
    );
    let mut stranded = v.clone();
    stranded["lifecycles"][0]["transitions"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    assert!(check(&stranded).is_err());
    for (key, value) in [
        ("guards", json!(["sql:select"])),
        ("guards", json!(vec!["payment.ready"; 5])),
        ("requiredPermissions", json!(["owner"])),
        ("requiredPermissions", json!(vec!["documents.read"; 9])),
    ] {
        let mut bad = v.clone();
        bad["lifecycles"][0]["transitions"][1][key] = value;
        assert!(check(&bad).is_err());
    }
    let mut duplicate = v.clone();
    duplicate["lifecycles"][0]["transitions"]
        .as_array_mut()
        .unwrap()
        .push(v["lifecycles"][0]["transitions"][1].clone());
    assert!(check(&duplicate).is_err());
    let mut h = host();
    h.scopes.clear();
    assert!(validate(&package(&v, &c, &p), &h).is_err());
    let mut h = host();
    h.locales = false;
    assert!(validate(&package(&v, &c, &p), &h).is_err());
}
#[test]
fn configured_main_language_is_explicit_and_empty_exact_label_blocks_fallback() {
    let v = fixture();
    let p = vec!["commerce.lifecycle".into()];
    let mut c = contracts(&v);
    validate_locale(&package(&v, &c, &p), "de-DE").unwrap();
    assert!(validate_locale(&package(&v, &c, &p), "it-IT").is_err());
    for contract in &mut c {
        for label in contract
            .states
            .iter_mut()
            .map(|s| &mut s.label)
            .chain(contract.transitions.iter_mut().map(|e| &mut e.label))
        {
            label.insert("it".into(), "Controllo".into());
        }
    }
    validate_locale(&package(&v, &c, &p), "it-IT").unwrap();
    c[0].states[0].label.insert("it-IT".into(), " ".into());
    assert!(validate_locale(&package(&v, &c, &p), "it-IT").is_err());
}

#[path = "native_lifecycle/snapshot.rs"]
mod snapshot;
