//! Test-only complete-input projection and explicit host policies; never a production package decoder or identity source.
use serde::Deserialize;
use serde_json::{Value, json};
use std::cell::RefCell;
use veyndo_app_model::{Action, Entity, Slot, declarations::ReferencePolicy, package::*};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    id: String,
    version: String,
    core_api: String,
    runtime: String,
    #[serde(default)]
    category: Option<String>,
    permissions: Vec<String>,
    entities: Vec<Entity>,
    actions: Vec<Action>,
    slots: Vec<Slot>,
    events: Vec<String>,
    #[serde(default)]
    configuration: Option<Config>,
    #[serde(default)]
    payment_provider: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    entity: String,
    price_field: String,
    input_field: String,
    record: String,
    wasm_source: String,
}
impl Input {
    pub fn contract(&self) -> Contract<'_> {
        Contract {
            id: &self.id,
            version: &self.version,
            core_api: &self.core_api,
            runtime: &self.runtime,
            category: self.category.as_deref(),
            permissions: &self.permissions,
            entities: &self.entities,
            actions: &self.actions,
            slots: &self.slots,
            events: &self.events,
            has_payment_provider: self.payment_provider.is_some(),
            configuration: self.configuration.as_ref().map(|c| Configuration {
                entity: &c.entity,
                price_field: &c.price_field,
                input_field: &c.input_field,
                record: &c.record,
                wasm_source: &c.wasm_source,
            }),
        }
    }
}
pub struct Policy;
impl ReferencePolicy for Policy {
    fn admitted(&self, product: bool, private: bool) -> bool {
        product || private
    }
}
impl veyndo_app_model::package::Policy for Policy {
    fn known_scope(&self, scope: &str) -> bool {
        [
            "apps.manage",
            "catalog.read",
            "orders.read",
            "customers.pii",
            "payments.manage",
        ]
        .contains(&scope)
    }
    fn component_source(&self, source: &str) -> bool {
        source == "(component)"
    }
}
pub struct Host {
    pub fail: &'static str,
    pub calls: RefCell<Vec<&'static str>>,
}
impl Host {
    pub fn new(fail: &'static str) -> Self {
        Self {
            fail,
            calls: RefCell::new(vec![]),
        }
    }
}
impl Extensions for Host {
    type Error = &'static str;
    fn before(&self) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push("before");
        if self.fail == "before" {
            Err("extension rejected before")
        } else {
            Ok(())
        }
    }
    fn after(&self) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push("after");
        if self.fail == "after" {
            Err("extension rejected after")
        } else {
            Ok(())
        }
    }
}
pub fn base() -> Value {
    json!({"id":"example","version":"1.0.0","coreApi":"1","runtime":"declarative","permissions":[],"entities":[],"actions":[],"slots":[],"events":[]})
}
pub fn action(handler: &str) -> Value {
    json!({"name":"execute","description":"Fixture","handler":handler,"public":false,"inputSchema":{"type":"object","additionalProperties":false}})
}
pub fn entity(public: bool) -> Value {
    json!({"name":"records","publicRead":public,"fields":[{"name":"title","kind":"string","required":true}]})
}
pub fn compile(value: &Value) -> Result<(), CompileError<&'static str>> {
    let input: Input = serde_json::from_value(value.clone()).unwrap();
    veyndo_app_model::package::compile(&input.contract(), &Policy, &Host::new(""))
}
pub fn rejected(value: &Value, text: &str) {
    match compile(value) {
        Err(CompileError::Model(e)) => {
            assert!(e.message.contains(text), "Expected {text}: {}", e.message)
        }
        other => panic!("Expected model rejection: {other:?}"),
    }
}
pub fn configured() -> Value {
    let mut m = base();
    m["permissions"] = json!(["data.read", "data.write"]);
    m["entities"] = json!([{"name":"rules","publicRead":false,"fields":[{"name":"fee","kind":"integer","required":true}]}]);
    m["configuration"] = json!({"entity":"rules","priceField":"fee","inputField":"message","record":"default","wasmSource":"(module)"});
    m
}
