//! Independently validate and execute the original synthetic business examples; no Core, SQL or paid model service is used.
use serde_json::Value;
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
    fn admitted(&self, product: bool, private: bool) -> bool {
        product || private
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let furniture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/business-furniture.json"))?;
    let entities = serde_json::from_value::<Vec<_>>(furniture["entities"].clone())?;
    let actions = serde_json::from_value::<Vec<_>>(furniture["actions"].clone())?;
    let config: configuration::Contract =
        serde_json::from_value(furniture["configuration"].clone())?;
    configuration::validate(
        &configuration::Context {
            entities: &entities,
            actions: &actions,
            configuration: Some(&config),
        },
        &Host,
    )?;
    let guest = Sandbox::new(&config.wasm_source, &Host, "")?;
    let inputs = config.inputs.as_ref().ok_or("Missing structured inputs")?;
    for case in &inputs.examples {
        let actual = configuration::arguments(&config, &case.fields, &Host)
            .ok()
            .and_then(|args| {
                execution::run(
                    &guest,
                    case.material_fee_minor,
                    Some(args),
                    inputs.arguments.len(),
                )
                .ok()
            });
        if actual != case.expected_fee_minor {
            return Err(format!("Configuration example {} failed", case.id).into());
        }
    }
    let snapshot: Value =
        serde_json::from_str(include_str!("../tests/fixtures/business-snapshot.json"))?;
    let entities = serde_json::from_value::<Vec<_>>(snapshot["entities"].clone())?;
    let permissions = serde_json::from_value::<Vec<String>>(snapshot["permissions"].clone())?;
    let hooks: commerce_hooks::Contract =
        serde_json::from_value(snapshot["commerceHooks"].clone())?;
    let capability: business_capability::Contract =
        serde_json::from_value(snapshot["capability"].clone())?;
    commerce_hooks::validate(
        &commerce_hooks::Context {
            entities: &entities,
            permissions: &permissions,
            commerce_hooks: Some(&hooks),
        },
        Sandbox::component_source,
    )?;
    business_capability::validate(
        &business_capability::Context {
            entities: &entities,
            commerce_hooks: Some(&hooks),
            capability: Some(&capability),
        },
        &Host,
    )?;
    let guest = Sandbox::new(&hooks.source, &Host, "")?;
    guest.validate_component_abi()?;
    business_examples::verify(Some(&capability), &guest)?;
    println!(
        "Validated and executed {} configuration and {} commerce examples",
        inputs.examples.len(),
        capability.examples.len()
    );
    Ok(())
}
