//! Independent package assembly checks preserve host validation order, budgets, grammar and reference/Wasm policy ports.
#[path = "support/package.rs"]
mod fixture;
use fixture::*;
use serde_json::json;
use veyndo_app_model::{
    Slot,
    package::{self, CompileError},
};
#[test]
fn host_extension_failures_cannot_be_skipped_and_keep_their_order() {
    let input: Input = serde_json::from_value(base()).unwrap();
    let before = Host::new("before");
    assert!(matches!(
        package::compile(&input.contract(), &Policy, &before),
        Err(CompileError::Host("extension rejected before"))
    ));
    assert_eq!(*before.calls.borrow(), ["before"]);
    let after = Host::new("after");
    assert!(matches!(
        package::compile(&input.contract(), &Policy, &after),
        Err(CompileError::Host("extension rejected after"))
    ));
    assert_eq!(*after.calls.borrow(), ["before", "after"]);
    let mut invalid = base();
    invalid["id"] = json!("../../escape");
    let input: Input = serde_json::from_value(invalid).unwrap();
    let host = Host::new("");
    assert!(matches!(
        package::compile(&input.contract(), &Policy, &host),
        Err(CompileError::Model(_))
    ));
    assert_eq!(*host.calls.borrow(), ["before"]);
}
#[test]
fn package_identity_runtime_permissions_and_declared_limits_are_closed() {
    compile(&base()).unwrap();
    for (key, value, error) in [
        ("id", json!("x;drop"), "Unsupported"),
        ("version", json!("1.2"), "Unsupported"),
        ("version", json!("1.2.x"), "Unsupported"),
        ("coreApi", json!("2"), "Unsupported"),
        ("runtime", json!("python"), "Unsupported"),
        ("category", json!("privileged"), "Unknown app category"),
        (
            "permissions",
            json!(["system.root"]),
            "Unknown app permission",
        ),
        (
            "permissions",
            json!(["events:bad/input"]),
            "Unknown app permission",
        ),
        ("permissions", json!(["events:"]), "Unknown app permission"),
        ("actions", json!(vec![action("service"); 25]), "Unsupported"),
        ("entities", json!(vec![entity(false); 13]), "Unsupported"),
        (
            "events",
            json!(vec!["app.example.changed"; 13]),
            "Unsupported",
        ),
    ] {
        let mut m = base();
        m[key] = value;
        rejected(&m, error);
    }
    let mut m = base();
    m["runtime"] = json!("service");
    m["permissions"] = json!(["events:self", "events:order.*", "customers.pii"]);
    m["events"] = json!(["order.placed"]);
    compile(&m).unwrap();
}
#[test]
fn total_declared_indexes_are_bounded_before_installing_models() {
    let mut m = base();
    let mut entities = vec![];
    for n in 0..8 {
        entities.push(json!({"name":format!("entity_{n}"),"fields":(0..8).map(|f|json!({"name":format!("field_{f}"),"kind":"string","indexed":true,"required":false})).collect::<Vec<_>>() }));
    }
    m["entities"] = json!(entities);
    compile(&m).unwrap();
    m["entities"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"extra","kind":"string","unique":true,"required":false}));
    rejected(&m, "Maximum 64 declared indexes");
}
#[test]
fn configuration_requires_declared_private_rules_and_wasm_policy() {
    let good = configured();
    compile(&good).unwrap();
    for (key, value) in [
        ("entity", json!("missing")),
        ("priceField", json!("title")),
        ("record", json!("")),
        ("wasmSource", json!("(component)")),
        ("wasmSource", json!("x".repeat(32769))),
    ] {
        let mut m = good.clone();
        m["configuration"][key] = value;
        rejected(&m, "Invalid app configuration contract");
    }
    let mut m = good.clone();
    m["permissions"] = json!(["data.read"]);
    rejected(&m, "Invalid app configuration contract");
    let mut m = good;
    m["entities"][0]["fields"][0]["kind"] = json!("string");
    rejected(&m, "Invalid app configuration contract");
}
#[test]
fn slots_retain_strict_wire_grammar_and_events_require_service_runtime() {
    let slot =
        json!({"location":"product.detail","component":"entity-list","label":{"de-DE":"Inhalte"}});
    let typed: Slot = serde_json::from_value(slot.clone()).unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), slot);
    let mut foreign = slot.clone();
    foreign["javascript"] = json!("run()");
    assert!(serde_json::from_value::<Slot>(foreign).is_err());
    let mut m = base();
    m["slots"] = json!([slot.clone()]);
    compile(&m).unwrap();
    m["slots"][0]["component"] = json!("javascript");
    rejected(&m, "Unsupported UI slot");
    m["slots"] = json!(vec![slot; 13]);
    rejected(&m, "Unsupported");
    let mut m = base();
    m["events"] = json!(["order.placed"]);
    rejected(&m, "Event subscriptions require service runtime");
    m["runtime"] = json!("service");
    m["events"] = json!(["order;drop"]);
    rejected(&m, "Invalid app event name");
}
#[test]
fn actions_require_known_scopes_unique_names_and_closed_bounded_schemas() {
    let mut m = base();
    m["actions"] = json!([action("service")]);
    compile(&m).unwrap();
    for (key, value) in [
        ("permission", json!("system.root")),
        ("requiredPermissions", json!(["system.root"])),
        ("requiredPermissions", json!(vec!["orders.read"; 9])),
        ("name", json!("../unsafe")),
        ("handler", json!("shell")),
        ("description", json!("x".repeat(301))),
    ] {
        let mut x = m.clone();
        x["actions"][0][key] = value;
        rejected(&x, "Invalid app action");
    }
    let mut x = m.clone();
    x["actions"].as_array_mut().unwrap().push(action("service"));
    rejected(&x, "Invalid app action");
    let mut x = m.clone();
    x["actions"][0]["inputSchema"]["additionalProperties"] = json!(true);
    rejected(&x, "Invalid app action");
    let mut x = m;
    x["actions"][0]["outputSchema"] = json!({"$ref":"https://attacker.test/schema"});
    assert!(compile(&x).is_err());
}
