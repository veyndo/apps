//! Acceptance examples exercise the same Wasm module as checkout, including changed inputs and failed-contract admission.
use super::*;
fn package() -> Manifest {
    serde_json::from_str(include_str!(
        "../../../extensions/apps/snapshot-pricing/manifest.json"
    ))
    .unwrap()
}
#[test]
fn frozen_decisions_execute_real_component_for_multiple_amounts_and_currency_scales() {
    let m = package();
    super::super::validate(&m).unwrap();
    let guest = Sandbox::new(&m.commerce_hooks.as_ref().unwrap().source).unwrap();
    guest.validate_component_abi().unwrap();
    verify(&m, &guest).unwrap();
    // A source or expectation change fails; merely storing examples cannot produce a pass.
    let mut wrong = m.clone();
    wrong.capability.as_mut().unwrap().examples[1]
        .expected
        .adjustment
        .minor += 1;
    let error = verify(&wrong, &guest).unwrap_err();
    assert_eq!(
        error.problem()["pointer"],
        "/capability/examples/1/expected"
    );
    let changed = Sandbox::new(
        &m.commerce_hooks
            .as_ref()
            .unwrap()
            .source
            .replace("i64.const 100 i64.div_s", "i64.const 50 i64.div_s"),
    )
    .unwrap();
    assert!(verify(&m, &changed).is_err());
    let trapped = Sandbox::new(&m.commerce_hooks.as_ref().unwrap().source.replace(
        "i32.const 256 call $cart",
        "(loop $again i32.const 256 call $cart br $again)",
    ))
    .unwrap();
    assert!(verify(&m, &trapped).is_err());
}
#[test]
fn frozen_contract_is_bounded_namespaced_and_never_reads_live_records() {
    for mutation in 0..7 {
        let mut m = package();
        let c = m.capability.as_mut().unwrap();
        match mutation {
            0 => c.version = 2,
            1 => c.examples[0].hook = "payment".into(),
            2 => c.examples.push(c.examples[0].clone()),
            3 => c.examples[0].snapshot.subtotal.currency = "eur".into(),
            4 => c.examples[0].snapshot.records.push(snapshot::Record {
                entity: "foreign".into(),
                id: "live-record".into(),
                revision: 1,
                fields: json!({}),
            }),
            5 => c.examples[0].context = json!({"oversized":"x".repeat(8193)}),
            _ => c.examples[0].expected.adjustment.scale = 3,
        }
        assert!(validate(&m).is_err(), "mutation {mutation}");
    }
    let mut legacy = json!(package());
    legacy.as_object_mut().unwrap().remove("capability");
    let m: Manifest = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(json!(m), legacy);
}
