//! Actual guest execution must retain mandatory resource admission, fresh stores and bounded snapshots independently of Core.
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use veyndo_app_runtime::{
    component_runtime::{CartSnapshot, Kind, Module, Money, Snapshot},
    policy::{ResourcePolicy, Resources},
    sandbox::Sandbox,
};
struct Host {
    admitted: bool,
    calls: AtomicUsize,
}
impl Host {
    fn new(admitted: bool) -> Self {
        Self {
            admitted,
            calls: AtomicUsize::new(0),
        }
    }
}
impl ResourcePolicy for Host {
    fn admitted(&self, r: Resources) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.admitted && r.tables <= 1 && r.memories <= 1 && r.elements <= 10000 && r.pages <= 16
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        cart: CartSnapshot {
            items: vec![],
            subtotal: Money {
                minor: 10000,
                currency: "EUR".into(),
                scale: 2,
            },
        },
        products: HashMap::new(),
        records: HashMap::new(),
    }
}
const WIT: &str = include_str!("fixtures/snapshot-price.component.wat");
#[test]
fn trusted_host_denial_prevents_core_and_component_construction() {
    let host = Host::new(false);
    assert!(Sandbox::new("(module)", &host, "").is_err());
    assert!(Sandbox::new(WIT, &host, "").is_err());
    assert!(Module::new(WIT, &host, "").is_err());
    assert_eq!(host.calls.load(Ordering::Relaxed), 3);
}
#[test]
fn guest_state_is_fresh_for_each_call_and_source_identity_is_exact() {
    let host = Host::new(true);
    let source = "(module (global $calls (mut i32) (i32.const 0)) (func (export \"approve\") (param i64 i64) (result i32) global.get $calls i32.const 1 i32.add global.set $calls global.get $calls i32.const 1 i32.eq))";
    let s = Sandbox::new(source, &host, "").unwrap();
    for _ in 0..10 {
        assert!(s.approve(100, 200).unwrap());
    }
    assert!(s.source_matches(source));
    assert!(!s.source_matches(&(source.to_owned() + " ")));
    let other = Sandbox::new(&(source.to_owned() + " "), &host, "").unwrap();
    assert_ne!(s.digest(), other.digest());
}
#[test]
fn snapshot_and_input_budgets_reject_without_changing_a_healthy_guest() {
    let module = Module::new(WIT, &Host::new(true), "").unwrap();
    assert_eq!(
        module
            .evaluate(snapshot(), Kind::Price, "{}")
            .unwrap()
            .adjustment
            .minor,
        100
    );
    assert!(
        module
            .evaluate(snapshot(), Kind::Price, &"x".repeat(8193))
            .is_err()
    );
    let mut wrong = snapshot();
    wrong.cart.subtotal.currency = "eur".into();
    assert!(module.evaluate(wrong, Kind::Price, "{}").is_err());
    let mut wrong = snapshot();
    wrong.cart.subtotal.scale = 4;
    assert!(module.evaluate(wrong, Kind::Price, "{}").is_err());
    let mut wrong = snapshot();
    for n in 0..33 {
        wrong.records.insert(
            ("notes".into(), n.to_string()),
            veyndo_app_runtime::component_runtime::AppRecord {
                id: n.to_string(),
                revision: 1,
                fields_json: "{}".into(),
            },
        );
    }
    assert!(module.evaluate(wrong, Kind::Price, "{}").is_err());
    assert_eq!(
        module
            .evaluate(snapshot(), Kind::Price, "{}")
            .unwrap()
            .adjustment
            .minor,
        100
    );
}
#[test]
fn configured_namespace_is_explicit_and_no_other_import_is_granted() {
    let old = WIT.replace("veyndo:commerce", "legacy:commerce");
    assert!(Module::new(&old, &Host::new(true), "").is_err());
    let module = Module::new(&old, &Host::new(true), "legacy").unwrap();
    module.validate_interface().unwrap();
    assert_eq!(
        module
            .evaluate(snapshot(), Kind::Price, "{}")
            .unwrap()
            .adjustment
            .minor,
        100
    );
    assert!(
        Sandbox::new(
            "(module (import \"wasi_snapshot_preview1\" \"fd_write\" (func)))",
            &Host::new(true),
            ""
        )
        .is_err()
    );
}
