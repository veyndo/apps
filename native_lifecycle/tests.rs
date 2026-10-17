//! Contract and native-boundary negative controls; operational tests exercise the actual owners separately.
use super::*;
fn package() -> Manifest {
    serde_json::from_str(include_str!(
        "../../../extensions/apps/production-review/manifest.json"
    ))
    .unwrap()
}
#[test]
fn requires_native_labels_in_the_configured_main_language() {
    let mut m = package();
    validate_locale(&m, "de-DE").unwrap();
    assert!(validate_locale(&m, "it-IT").is_err());
    for c in &mut m.lifecycles {
        for s in &mut c.states {
            s.label.insert("it".into(), "Controllo".into());
        }
        for e in &mut c.transitions {
            e.label.insert("it".into(), "Prosegui".into());
        }
    }
    validate_locale(&m, "it-IT").unwrap();
    m.lifecycles[0].states[0]
        .label
        .insert("it-IT".into(), " ".into());
    assert!(validate_locale(&m, "it-IT").is_err());
}
#[test]
fn package_local_graphs_cannot_replace_payment_or_buyer_acceptance() {
    let m = package();
    validate(&m).unwrap();
    for target in ["offered", "accepted", "ordered"] {
        let mut invalid = m.clone();
        invalid.lifecycles[1].transitions[1].to = target.into();
        assert!(validate(&invalid).is_err());
    }
    let mut special = commerce::default_machine();
    special.states.push(commerce::MachineState {
        id: "manual_archive".into(),
        label: m.lifecycles[0].states[0].label.clone(),
        terminal: true,
    });
    let mut invalid = m.clone();
    invalid.lifecycles[0].transitions[1].to = "manual_archive".into();
    assert!(compile(special, &[invalid], "order").is_err());
    for replace in ["cancel_new", "payments.capture", "unknown"] {
        let mut invalid = m.clone();
        invalid.lifecycles[0].transitions[0].replaces = Some(replace.into());
        assert!(validate(&invalid).is_err());
    }
    let mut invalid = m.clone();
    invalid.lifecycles[0].transitions[1].from = "completed".into();
    assert!(validate(&invalid).is_err());
}
#[test]
fn install_cannot_compete_strand_or_execute_unbounded_guards() {
    let m = package();
    let mut other = m.clone();
    other.id = "competitor".into();
    assert!(compile(commerce::default_machine(), &[m.clone(), other], "order").is_err());
    let mut invalid = m.clone();
    invalid.lifecycles[0].transitions.truncate(1);
    assert!(validate(&invalid).is_err());
    let mut invalid = m.clone();
    invalid.lifecycles[0].transitions[1]
        .guards
        .push("sql:select".into());
    assert!(validate(&invalid).is_err());
    let mut invalid = m.clone();
    invalid.lifecycles[0].transitions[1]
        .required_permissions
        .push("owner".into());
    assert!(validate(&invalid).is_err());
    let mut invalid = m.clone();
    invalid.permissions.clear();
    assert!(validate(&invalid).is_err());
}
#[test]
fn namespaced_graph_and_receipt_keep_exact_accepted_version() {
    let m = package();
    let machine = compile(
        commerce::default_machine(),
        std::slice::from_ref(&m),
        "order",
    )
    .unwrap();
    assert!(!machine.transitions.iter().any(|e| e.id == "complete"));
    let e = machine
        .transitions
        .iter()
        .find(|e| e.id.ends_with("_approve"))
        .unwrap();
    assert_eq!(
        e.policy.as_ref().unwrap().digest,
        approval::canonical_digest(&m)
    );
    assert_eq!(e.policy.as_ref().unwrap().version, "1.0.0");
    assert!(identifier(&e.from));
    assert!(identifier(&e.id));
    let order = json!({"nativeLifecycle":Snapshot {version:1,object:"order".into(),revision:3,main_locale:"it-IT".into(),machine}});
    assert_eq!(
        snapshot(&order, "order").unwrap().unwrap().main_locale,
        "it-IT"
    );
    assert!(snapshot(&order, "quote").is_err());
    assert!(snapshot(&json!({}), "order").unwrap().is_none());
}
#[test]
fn app_guards_only_narrow_and_never_invent_payment_or_delivery_truth() {
    for payment in ["pending", "authorized", "paid", "captured", "refunded"] {
        let data = json!({"payment":{"provider":"manual","state":payment},"deliveries":[{"state":"open"}]});
        assert_eq!(
            fact_value("payment.ready", &data),
            ["paid", "captured"].contains(&payment)
        );
        assert!(!fact_value("delivery.delivered", &data));
        assert!(!allowed(false, true, true));
        assert!(!allowed(true, false, true));
    }
    assert!(!fact_value("unknown", &json!({})));
    assert!(!fact_value("quote.message", &json!({"message":"  "})));
    assert!(fact_value(
        "quote.message",
        &json!({"message":"Material check"})
    ));
}
