//! Frozen graph wire, public projection and current-right/fact regressions.
use super::*;
#[test]
fn accepted_snapshots_survive_package_changes_without_revealing_policies() {
    let v = fixture();
    let c = contracts(&v);
    let p = vec!["commerce.lifecycle".into()];
    let graph = compile(
        host().order_base(),
        &[package(&v, &c, &p)],
        "order",
        &host(),
    )
    .unwrap();
    let mut data = json!({"nativeLifecycle":Snapshot{version:1,object:"order".into(),revision:3,main_locale:"it-IT".into(),machine:graph.clone()}});
    let (actual, revision) = accepted::order_machine(&data, quote_machine(), 99).unwrap();
    assert_eq!(json!(actual), json!(graph));
    assert_eq!(revision, 3);
    let mut public = json!({});
    accepted::project_labels(&data, &mut public);
    assert_eq!(public["stateMainLocale"], "it-IT");
    assert!(public["stateLabels"].get("placed").is_some());
    assert!(!public.to_string().contains("digest"));
    assert!(!public.to_string().contains("requiredPermissions"));
    assert_eq!(
        accepted::snapshot(&data, "quote").err().unwrap().kind,
        Kind::Conflict
    );
    data["nativeLifecycle"]["version"] = json!(2);
    assert_eq!(
        accepted::snapshot(&data, "order").err().unwrap().kind,
        Kind::Conflict
    );
    data["nativeLifecycle"] = json!({"version":1,"untrusted":"shell"});
    assert_eq!(
        accepted::snapshot(&data, "order").err().unwrap().kind,
        Kind::Conflict
    );
    for data in [json!({}), json!({"nativeLifecycle":null})] {
        let (m, r) = accepted::order_machine(&data, host().order_base(), 7).unwrap();
        assert_eq!(r, 7);
        assert_eq!(json!(m), json!(host().order_base()));
        assert!(accepted::state_labels(&data).is_none());
    }
}
#[test]
fn strict_wire_keeps_original_defaults_and_rejects_executable_metadata() {
    let v = fixture();
    let c = contracts(&v);
    assert_eq!(json!(c), v["lifecycles"]);
    let machine = host().order_base();
    assert!(json!(machine)["sourceApp"].is_null());
    assert!(json!(machine)["transitions"][0].get("policy").is_none());
    for type_name in ["state", "edge", "contract", "policy", "snapshot"] {
        let mut raw = match type_name {
            "state" => json!(machine.states[0]),
            "edge" => json!(machine.transitions[0]),
            "contract" => json!(c[0]),
            "policy" => {
                json!({"app":"a","version":"1","digest":"x","requiredPermissions":[],"guards":[]})
            }
            _ => {
                json!({"version":1,"object":"order","revision":0,"mainLocale":"de-DE","machine":machine})
            }
        };
        raw["shell"] = json!("exec");
        let rejected = match type_name {
            "state" => serde_json::from_value::<MachineState>(raw).is_err(),
            "edge" => serde_json::from_value::<MachineEdge>(raw).is_err(),
            "contract" => serde_json::from_value::<Contract>(raw).is_err(),
            "policy" => serde_json::from_value::<Policy>(raw).is_err(),
            _ => serde_json::from_value::<Snapshot>(raw).is_err(),
        };
        assert!(rejected);
    }
}
#[test]
fn current_authority_and_actual_facts_can_only_narrow_graph_eligibility() {
    let v = fixture();
    let c = contracts(&v);
    let p = vec!["commerce.lifecycle".into()];
    let graph = compile(
        host().order_base(),
        &[package(&v, &c, &p)],
        "order",
        &host(),
    )
    .unwrap();
    let edge = graph
        .transitions
        .iter()
        .find(|e| e.id.ends_with("_approve"))
        .unwrap();
    let mut data = json!({"payment":{"provider":"manual","state":"paid"},"deliveries":[{"state":"delivered"}]});
    assert_eq!(reason(edge, &data, |_| false), Some("lifecyclePermission"));
    assert_eq!(reason(edge, &data, |s| s == "documents.read"), None);
    for state in ["pending", "authorized", "refunded"] {
        data["payment"]["state"] = json!(state);
        assert_eq!(reason(edge, &data, |_| true), Some("lifecycleGuard"));
    }
    data["payment"]["provider"] = json!("simulated");
    data["payment"]["state"] = json!("authorized");
    assert_eq!(reason(edge, &data, |_| true), None);
    data["deliveries"][0]["state"] = json!("open");
    assert_eq!(reason(edge, &data, |_| true), Some("lifecycleGuard"));
    assert!(!fact_value("payment.ready", &json!({})));
    assert!(!fact_value("delivery.delivered", &json!({})));
    assert!(!fact_value("unknown", &data));
    assert!(!fact_value("quote.message", &json!({"message":"  "})));
    assert!(fact_value("quote.message", &json!({"message":"Review"})));
    assert_eq!(
        reason(&host().order_base().transitions[0], &json!({}), |_| false),
        None
    );
}
