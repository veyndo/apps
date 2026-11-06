//! Explicit synthetic host for the independent full-compiler example/tests; unsupported values and all publisher signatures fail closed.
use serde_json::Value;
use std::cell::RefCell;
use veyndo_app_model::{self as model, Action, manifest::Manifest};
use veyndo_app_runtime::sandbox::Sandbox;
pub struct Host {
    pub calls: RefCell<Vec<&'static str>>,
    pub deny: &'static str,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            calls: RefCell::new(vec![]),
            deny: "",
        }
    }
}
impl Host {
    fn require(&self, stage: &'static str) -> Result<(), &'static str> {
        self.calls.borrow_mut().push(stage);
        if self.deny == stage {
            Err(stage)
        } else {
            Ok(())
        }
    }
}
impl model::fields::ValuePolicy for Host {
    fn locale(&self, key: &str) -> bool {
        !key.is_empty()
            && key.len() <= 35
            && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
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
impl model::declarations::ReferencePolicy for Host {
    fn admitted(&self, product: bool, private: bool) -> bool {
        self.deny != "reference" && (product || private)
    }
}
impl model::configuration::Policy for Host {
    fn known_currency(&self, code: &str) -> bool {
        self.deny != "currency" && code == "EUR"
    }
}
const SCOPES: &[&str] = &[
    "overview.read",
    "assistant.use",
    "automation.read",
    "automation.manage",
    "channels.read",
    "channels.manage",
    "developer.read",
    "developer.manage",
    "environments.read",
    "environments.manage",
    "knowledge.write",
    "ai.manage",
    "apps.read",
    "pos.use",
    "b2b.read",
    "b2b.manage",
    "catalog.read",
    "catalog.write",
    "orders.read",
    "orders.write",
    "customers.read",
    "customers.pii",
    "customers.write",
    "documents.read",
    "documents.create",
    "payments.read",
    "payments.manage",
    "team.manage",
    "settings.read",
    "settings.write",
    "apps.manage",
    "knowledge.read",
];
impl model::package::Policy for Host {
    fn known_scope(&self, scope: &str) -> bool {
        self.deny != "scope" && SCOPES.contains(&scope)
    }
    fn component_source(&self, source: &str) -> bool {
        Sandbox::component_source(source)
    }
}
impl model::views::Policy for Host {
    fn public_read(&self, writes: bool) -> bool {
        self.deny != "public_read" && !writes
    }
}
impl model::native_lifecycle::CompilerPolicy for Host {
    fn scopes(&self) -> &[&str] {
        if self.deny == "scope" { &[] } else { SCOPES }
    }
    fn locale(&self, key: &str) -> bool {
        model::fields::ValuePolicy::locale(self, key)
    }
    fn order_base(&self) -> model::native_lifecycle::Machine {
        let v: Value = serde_json::from_str(include_str!(
            "../../../model/tests/fixtures/native_lifecycle.json"
        ))
        .unwrap();
        serde_json::from_value(v["orderBase"].clone()).unwrap()
    }
}
impl model::distribution::SignatureVerifier for Host {
    type Error = &'static str;
    fn verify_signature(&self, _: &Manifest) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push("signature");
        Err("no registered publisher")
    }
}
impl model::compiler::Policy for Host {
    fn unsigned_namespace(&self, m: &Manifest) -> Result<(), Self::Error> {
        self.require("namespace")?;
        if m.distribution.is_none() && m.id.starts_with("reserved_") {
            Err("reserved namespace")
        } else {
            Ok(())
        }
    }
    fn destination(&self, url: &str) -> Result<(), Self::Error> {
        self.require("destination")?;
        if url == "https://events.example.test/events" {
            Ok(())
        } else {
            Err("unapproved destination")
        }
    }
    fn readonly(&self, action: &Action) -> bool {
        self.deny != "readonly"
            && (action.read_only || ["list", "configurations"].contains(&action.handler.as_str()))
            && !["save", "emit", "submit"].contains(&action.handler.as_str())
    }
    fn now(&self) -> model::compiler::DateTime<model::compiler::Utc> {
        self.calls.borrow_mut().push("clock");
        model::compiler::DateTime::from_timestamp(1_794_000_000, 0).unwrap()
    }
}
pub fn fixtures() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../../../model/tests/fixtures/complete_packages.json"
    ))
    .unwrap()
}
pub fn package(id: &str) -> Manifest {
    serde_json::from_value(
        fixtures()
            .into_iter()
            .find(|v| v["manifest"]["id"] == id)
            .unwrap()["manifest"]
            .clone(),
    )
    .unwrap()
}
