//! Validate standalone placement and route declarations with explicit host policies; this example does not serve or render an app.
use serde_json::json;
use veyndo_app_model::{
    Action,
    surfaces::{self, ApiRoute, Contract, Policy, Surface},
};
struct Host;
impl Policy for Host {
    fn native_view(&self, s: &Surface) -> bool {
        ["native/manager", "native/summary"].contains(&s.ui_path.as_str())
    }
    fn known_scope(&self, s: &str) -> bool {
        s == "catalog.read"
    }
    fn readonly(&self, a: &Action) -> bool {
        a.name == "read" && a.handler == "list"
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let actions: Vec<Action> = serde_json::from_value(json!([
    {"name":"read","description":"Read","handler":"list","public":true,"inputSchema":{"type":"object"}},
    {"name":"save","description":"Save","handler":"save","public":false,"inputSchema":{"type":"object"}}
    ]))?;
    let surfaces: Vec<Surface> = serde_json::from_value(json!([
    {"id":"manager","location":"admin.navigation","label":{"en-GB":"Manager","de-DE":"Verwaltung"},"uiPath":"native/manager","actions":["read","save"],"permission":"catalog.read"},
    {"id":"summary","location":"product.detail","label":{"en-GB":"Summary"},"uiPath":"native/summary","actions":["read"]}
    ]))?;
    let routes: Vec<ApiRoute> = serde_json::from_value(json!([
    {"path":"entries","method":"GET","scope":"storefront","action":"read"},
    {"path":"entries","method":"POST","scope":"admin","action":"save"}
    ]))?;
    let permissions = vec!["admin.slot".into(), "storefront.slot".into()];
    let contract = Contract {
        runtime: "declarative",
        permissions: &permissions,
        actions: &actions,
        surfaces: &surfaces,
        api_routes: &routes,
    };
    surfaces::validate(&contract, &Host)?;
    println!(
        "Validated {} placements and {} named routes",
        surfaces.len(),
        routes.len()
    );
    let mut unsafe_routes = routes.clone();
    unsafe_routes[1].method = "GET".into();
    assert!(
        surfaces::validate(
            &Contract {
                api_routes: &unsafe_routes,
                ..contract
            },
            &Host
        )
        .is_err()
    );
    Ok(())
}
