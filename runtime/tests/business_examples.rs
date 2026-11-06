//! Shared business declarations execute actual guests and reject changed expected outcomes and malformed configuration.
use serde_json::{Value, json};
use veyndo_app_model::{
    business_capability, commerce_hooks, configuration, declarations::ReferencePolicy,
    fields::ValuePolicy,
};
use veyndo_app_runtime::{
    business_examples, configuration as execution,
    policy::{ResourcePolicy, Resources},
    sandbox::Sandbox,
};
struct Host;
impl ValuePolicy for Host {
    fn locale(&self, k: &str) -> bool {
        !k.is_empty() && k.len() <= 35 && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }
    fn money(&self, _: &Value) -> bool {
        false
    }
    fn richtext(&self, _: &Value) -> bool {
        false
    }
    fn relations(&self, _: &Value) -> bool {
        false
    }
}
impl ReferencePolicy for Host {
    fn admitted(&self, p: bool, private: bool) -> bool {
        p || private
    }
}
impl configuration::Policy for Host {
    fn known_currency(&self, c: &str) -> bool {
        c == "EUR"
    }
}
impl ResourcePolicy for Host {
    fn admitted(&self, r: Resources) -> bool {
        r.tables <= 1 && r.memories <= 1 && r.elements <= 10000 && r.pages <= 16
    }
}
#[test]
fn independent_preflight_executes_frozen_commerce_cases_and_rejects_changed_guests() {
    let v: Value = serde_json::from_str(include_str!("fixtures/business-snapshot.json")).unwrap();
    let entities =
        serde_json::from_value::<Vec<veyndo_app_model::Entity>>(v["entities"].clone()).unwrap();
    let permissions = serde_json::from_value::<Vec<String>>(v["permissions"].clone()).unwrap();
    let hooks: commerce_hooks::Contract =
        serde_json::from_value(v["commerceHooks"].clone()).unwrap();
    let cap: business_capability::Contract =
        serde_json::from_value(v["capability"].clone()).unwrap();
    commerce_hooks::validate(
        &commerce_hooks::Context {
            entities: &entities,
            permissions: &permissions,
            commerce_hooks: Some(&hooks),
        },
        Sandbox::component_source,
    )
    .unwrap();
    business_capability::validate(
        &business_capability::Context {
            entities: &entities,
            commerce_hooks: Some(&hooks),
            capability: Some(&cap),
        },
        &Host,
    )
    .unwrap();
    let guest = Sandbox::new(&hooks.source, &Host, "").unwrap();
    guest.validate_component_abi().unwrap();
    business_examples::verify(Some(&cap), &guest).unwrap();
    let mut wrong = cap.clone();
    wrong.examples[1].expected.adjustment.minor += 1;
    assert_eq!(
        business_examples::verify(Some(&wrong), &guest)
            .unwrap_err()
            .pointer
            .as_deref(),
        Some("/capability/examples/1/expected")
    );
    let changed = Sandbox::new(
        &hooks
            .source
            .replace("i64.const 100 i64.div_s", "i64.const 50 i64.div_s"),
        &Host,
        "",
    )
    .unwrap();
    assert!(business_examples::verify(Some(&cap), &changed).is_err());
    let trapped = Sandbox::new(
        &hooks.source.replace(
            "i32.const 256 call $cart",
            "(loop $again i32.const 256 call $cart br $again)",
        ),
        &Host,
        "",
    )
    .unwrap();
    assert!(business_examples::verify(Some(&cap), &trapped).is_err());
}
#[test]
fn frozen_configuration_matches_the_actual_guest_for_acceptance_and_rejection() {
    let v: Value = serde_json::from_str(include_str!("fixtures/business-furniture.json")).unwrap();
    let entities =
        serde_json::from_value::<Vec<veyndo_app_model::Entity>>(v["entities"].clone()).unwrap();
    let actions =
        serde_json::from_value::<Vec<veyndo_app_model::Action>>(v["actions"].clone()).unwrap();
    let c: configuration::Contract = serde_json::from_value(v["configuration"].clone()).unwrap();
    configuration::validate(
        &configuration::Context {
            entities: &entities,
            actions: &actions,
            configuration: Some(&c),
        },
        &Host,
    )
    .unwrap();
    let guest = Sandbox::new(&c.wasm_source, &Host, "").unwrap();
    for example in &c.inputs.as_ref().unwrap().examples {
        let actual = configuration::arguments(&c, &example.fields, &Host)
            .ok()
            .and_then(|args| {
                execution::run(
                    &guest,
                    example.material_fee_minor,
                    Some(args),
                    c.inputs.as_ref().unwrap().arguments.len(),
                )
                .ok()
            });
        assert_eq!(actual, example.expected_fee_minor, "{}", example.id);
    }
    let fields = json!({"material":"oak","width_cm":260,"depth_cm":80});
    assert_eq!(
        execution::run(
            &guest,
            0,
            Some(configuration::arguments(&c, &fields, &Host).unwrap()),
            2
        )
        .unwrap(),
        4800
    );
    assert_eq!(
        execution::run(&guest, 2000, Some(vec![240, 80]), 2).unwrap(),
        2000
    );
    assert!(execution::run(&guest, -1, Some(vec![240, 80]), 2).is_err());
}
