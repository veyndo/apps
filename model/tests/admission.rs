//! Trusted-host action/grant ports are exercised with revocation, compound rights and untrusted body fields.
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use veyndo_app_model::{
    Action, Kind,
    admission::{self, AdmissionError, Authority, ScopeAuthority},
};
struct Host {
    merchant: Cell<bool>,
    grants: RefCell<HashSet<String>>,
    calls: RefCell<Vec<String>>,
}
impl Host {
    fn new(grants: &[&str]) -> Self {
        Self {
            merchant: Cell::new(true),
            grants: RefCell::new(grants.iter().map(|s| s.to_string()).collect()),
            calls: RefCell::new(vec![]),
        }
    }
}
impl ScopeAuthority for Host {
    type Error = &'static str;
    fn require_scope(&self, scope: &str) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push(scope.into());
        if self.grants.borrow().contains(scope) {
            Ok(())
        } else {
            Err("current scope denied")
        }
    }
}
impl Authority for Host {
    fn require_merchant(&self) -> Result<(), Self::Error> {
        if self.merchant.get() {
            Ok(())
        } else {
            Err("merchant identity denied")
        }
    }
}
fn action(handler: &str) -> Action {
    serde_json::from_value(json!({"name":"run","description":"Fixture","handler":handler,"inputSchema":{"type":"object","additionalProperties":false},"public":false})).unwrap()
}
#[test]
fn merchant_and_current_compound_rights_precede_input_and_are_not_cached() {
    let host = Host::new(&["catalog", "customers.pii"]);
    let mut a = action("save");
    a.required_permissions = vec!["customers.pii".into()];
    admission::check(&host, &a, &json!({})).unwrap();
    host.grants.borrow_mut().remove("customers.pii");
    assert!(matches!(
        admission::check(&host, &a, &json!({})),
        Err(AdmissionError::Authority("current scope denied"))
    ));
    host.merchant.set(false);
    host.calls.borrow_mut().clear();
    assert!(matches!(
        admission::check(&host, &a, &json!({"role":"owner"})),
        Err(AdmissionError::Authority("merchant identity denied"))
    ));
    assert!(host.calls.borrow().is_empty());
}
#[test]
fn legacy_write_defaults_and_explicit_primary_scope_are_preserved() {
    for handler in ["save", "service", "emit", "job"] {
        let required = if handler == "job" {
            "apps.manage"
        } else {
            "catalog"
        };
        admission::check(&Host::new(&[required]), &action(handler), &json!({})).unwrap();
        assert!(matches!(
            admission::check(&Host::new(&[]), &action(handler), &json!({})),
            Err(AdmissionError::Authority(_))
        ));
    }
    let mut a = action("list");
    a.permission = Some("orders.read".into());
    assert!(admission::check(&Host::new(&[]), &a, &json!({})).is_err());
    admission::check(&Host::new(&["orders.read"]), &a, &json!({})).unwrap();
}
#[test]
fn public_scope_does_not_skip_extra_order_or_declared_permissions() {
    let host = Host::new(&[]);
    host.merchant.set(false);
    let mut a = action("configurations");
    a.public = true;
    assert!(matches!(
        admission::check(&host, &a, &json!({})),
        Err(AdmissionError::Authority(_))
    ));
    host.grants.borrow_mut().insert("orders.read".into());
    admission::check(&host, &a, &json!({})).unwrap();
    a.required_permissions = vec!["customers.pii".into()];
    assert!(admission::check(&host, &a, &json!({})).is_err());
}
#[test]
fn untrusted_body_claims_cannot_grant_scope_and_validation_keeps_pointers() {
    let body = json!({"permissions":["catalog"],"role":"owner","tenant":"foreign"});
    assert!(matches!(
        admission::check(&Host::new(&[]), &action("save"), &body),
        Err(AdmissionError::Authority(_))
    ));
    let error = admission::check(&Host::new(&["catalog"]), &action("save"), &body).unwrap_err();
    let AdmissionError::Model(error) = error else {
        panic!("expected model error")
    };
    assert_eq!(error.kind, Kind::Validation);
    assert!(error.pointer.is_some());
}
#[test]
fn callback_consent_and_role_are_separate_and_unknown_capabilities_fail_closed() {
    let grants = vec!["data.read".into()];
    assert_eq!(
        admission::callback_facts(true, Some(&grants), "data.read", |s| s == "apps.manage"),
        (true, true)
    );
    assert_eq!(
        admission::callback_facts(true, Some(&grants), "data.write", |_| true),
        (false, true)
    );
    assert_eq!(
        admission::callback_facts(true, None, "data.read", |_| true),
        (false, true)
    );
    assert_eq!(
        admission::callback_facts(true, Some(&grants), "data.read", |_| false),
        (true, false)
    );
    assert_eq!(
        admission::callback_facts(false, None, "data.read", |_| true),
        (true, true)
    );
    assert_eq!(
        admission::callback_facts(false, None, "unknown", |_| panic!(
            "unknown scope must not be delegated"
        )),
        (true, false)
    );
}
#[test]
fn requested_keys_require_declared_consent_current_scopes_and_explicit_pii_pairing() {
    let declared =
        ["data.read", "jobs.write", "customers.pii", "customers.read"].map(str::to_owned);
    let host = Host::new(&["apps.manage", "customers.pii", "customers.read"]);
    assert_eq!(
        admission::requested_scopes(&host, &declared, &json!(["data.read", "jobs.write"])).unwrap(),
        ["apps.manage"]
    );
    assert!(admission::requested_scopes(&host, &declared, &json!(["customers.pii"])).is_err());
    assert!(admission::requested_scopes(&host, &declared, &json!(["orders.import"])).is_err());
    assert!(admission::requested_scopes(&host, &declared, &json!([])).is_err());
    let too_many = Value::Array(vec![json!("data.read"); 33]);
    assert!(admission::requested_scopes(&host, &declared, &too_many).is_err());
    assert_eq!(
        admission::requested_scopes(
            &host,
            &declared,
            &json!(["customers.pii", "customers.read"])
        )
        .unwrap(),
        ["customers.pii", "customers.read"]
    );
    host.grants.borrow_mut().remove("customers.pii");
    assert!(matches!(
        admission::requested_scopes(
            &host,
            &declared,
            &json!(["customers.pii", "customers.read"])
        ),
        Err(AdmissionError::Authority(_))
    ));
}
#[test]
fn callback_mapping_retains_financial_and_pii_scopes() {
    for (capability, scope) in [
        ("orders.import", "orders.write"),
        ("payments.record", "payments.manage"),
        ("documents.create", "documents.create"),
        ("customers.pii", "customers.pii"),
        ("products.write", "catalog.write"),
        ("channels.read", "settings.read"),
        ("settings.read", "team.manage"),
    ] {
        assert_eq!(admission::callback_scope(capability), Some(scope));
    }
    assert_eq!(admission::callback_scope("admin"), None);
}
#[test]
fn action_wire_defaults_do_not_add_approval_metadata() {
    let wire = serde_json::to_value(action("list")).unwrap();
    for name in [
        "readOnly",
        "permission",
        "requiredPermissions",
        "outputSchema",
        "mcp",
        "flowAllowed",
    ] {
        assert!(wire.get(name).is_none(), "{name}");
    }
}
