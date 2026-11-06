//! Compile a bounded app graph and accepted receipt without Core/SQL; the host still owns native states, current rights and effects.
use serde_json::{Value, json};
use veyndo_app_model::{native_lifecycle::*, native_lifecycle_snapshot};
struct Host(Machine);
impl CompilerPolicy for Host {
    fn scopes(&self) -> &[&str] {
        &["documents.read"]
    }
    fn locale(&self, locale: &str) -> bool {
        ["en-GB", "de-DE", "fr-FR", "es-ES"].contains(&locale)
    }
    fn order_base(&self) -> Machine {
        self.0.clone()
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Synthetic example with the original production-review declarations. In a real
    // host, digest comes from the complete admitted package, never this projection.
    let v: Value = serde_json::from_str(include_str!(
        "../model/tests/fixtures/native_lifecycle.json"
    ))?;
    let contracts: Vec<Contract> = serde_json::from_value(v["lifecycles"].clone())?;
    let permissions = vec!["commerce.lifecycle".into()];
    let package = Package {
        id: v["id"].as_str().unwrap(),
        version: v["version"].as_str().unwrap(),
        digest: v["digest"].as_str().unwrap(),
        permissions: &permissions,
        lifecycles: &contracts,
    };
    let host = Host(serde_json::from_value(v["orderBase"].clone())?);
    validate(&package, &host)?;
    validate_locale(&package, "de-DE")?;
    let order = compile(
        host.order_base(),
        std::slice::from_ref(&package),
        "order",
        &host,
    )?;
    let quote = compile(quote_machine(), &[package], "quote", &host)?;
    let approve = order
        .transitions
        .iter()
        .find(|e| e.id.ends_with("_approve"))
        .unwrap();
    let facts = json!({"payment":{"provider":"manual","state":"paid"},"deliveries":[{"state":"delivered"}]});
    assert_eq!(reason(approve, &facts, |s| s == "documents.read"), None);
    assert_eq!(
        reason(approve, &facts, |_| false),
        Some("lifecyclePermission")
    );
    let receipt = json!({"nativeLifecycle":Snapshot {version:1,object:"order".into(),revision:3,main_locale:"de-DE".into(),machine:order}});
    let mut public = json!({});
    native_lifecycle_snapshot::project_labels(&receipt, &mut public);
    assert!(public.get("nativeLifecycle").is_none());
    println!(
        "Compiled order review and {} quote-preparation actions; accepted labels retained; revoked permission rejected",
        quote.transitions.len()
    );
    Ok(())
}
