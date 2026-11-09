//! Placement/route admission scenarios keep public/private actions, explicit host authority and precise native problem pointers.
use serde_json::json;
use std::cell::Cell;
use veyndo_app_model::{
    Action, Kind,
    surfaces::{self, ApiRoute, Contract, Policy, Surface},
};
struct Host {
    views: bool,
    scopes: bool,
    read: bool,
    view_calls: Cell<usize>,
    read_calls: Cell<usize>,
}
impl Host {
    fn new() -> Self {
        Self {
            views: true,
            scopes: true,
            read: true,
            view_calls: Cell::new(0),
            read_calls: Cell::new(0),
        }
    }
}
impl Policy for Host {
    fn native_view(&self, _: &Surface) -> bool {
        self.view_calls.set(self.view_calls.get() + 1);
        self.views
    }
    fn known_scope(&self, _: &str) -> bool {
        self.scopes
    }
    fn readonly(&self, _: &Action) -> bool {
        self.read_calls.set(self.read_calls.get() + 1);
        self.read
    }
}
struct Fixture {
    runtime: String,
    permissions: Vec<String>,
    actions: Vec<Action>,
    surfaces: Vec<Surface>,
    routes: Vec<ApiRoute>,
}
impl Fixture {
    fn new() -> Self {
        Self{runtime:"declarative".into(),permissions:vec!["admin.slot".into(),"storefront.slot".into()],actions:serde_json::from_value(json!([
 {"name":"read","description":"Read","handler":"list","public":true,"inputSchema":{"type":"object"}},
 {"name":"write","description":"Write","handler":"save","public":false,"inputSchema":{"type":"object"}}
 ])).unwrap(),surfaces:serde_json::from_value(json!([{"id":"manager","location":"admin.navigation","label":{"de-DE":"Verwaltung","it-IT":"Gestione"},"uiPath":"native/manager","actions":["read","write"]}])).unwrap(),routes:vec![]}
    }
    fn contract(&self) -> Contract<'_> {
        Contract {
            runtime: &self.runtime,
            permissions: &self.permissions,
            actions: &self.actions,
            surfaces: &self.surfaces,
            views: &[],
            api_routes: &self.routes,
        }
    }
}
fn route(scope: &str, method: &str, action: &str) -> ApiRoute {
    ApiRoute {
        path: "entries".into(),
        scope: scope.into(),
        method: method.into(),
        action: action.into(),
    }
}
#[test]
fn strict_wire_preserves_defaults_and_rejects_unknown_security_fields() {
    let raw = json!({"id":"manager","location":"admin.navigation","label":{"de-DE":"Verwaltung"},"uiPath":"native/manager"});
    let parsed: Surface = serde_json::from_value(raw.clone()).unwrap();
    let mut expected = raw.clone();
    expected["actions"] = json!([]);
    assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    let mut injected = raw;
    injected["token"] = json!("x");
    assert!(serde_json::from_value::<Surface>(injected).is_err());
    let mut raw = serde_json::to_value(route("admin", "GET", "read")).unwrap();
    assert_eq!(
        raw,
        json!({"path":"entries","scope":"admin","method":"GET","action":"read"})
    );
    raw["permission"] = json!("admin");
    assert!(serde_json::from_value::<ApiRoute>(raw).is_err());
}
#[test]
fn public_surfaces_cannot_expose_private_or_unknown_actions() {
    let mut f = Fixture::new();
    f.surfaces[0].location = "product.detail".into();
    let e = surfaces::validate(&f.contract(), &Host::new()).unwrap_err();
    assert_eq!(e.kind, Kind::Validation);
    assert_eq!(e.pointer.as_deref(), Some("/surfaces/0/actions/1"));
    f.surfaces[0].actions = vec!["missing".into()];
    let e = surfaces::validate(&f.contract(), &Host::new()).unwrap_err();
    assert_eq!(e.pointer.as_deref(), Some("/surfaces/0/actions/0"));
    f.surfaces[0].actions = vec!["read".into()];
    surfaces::validate(&f.contract(), &Host::new()).unwrap();
    f.surfaces[0].permission = Some("catalog.read".into());
    let e = surfaces::validate(&f.contract(), &Host::new()).unwrap_err();
    assert_eq!(e.pointer.as_deref(), Some("/surfaces/0/permission"));
}
#[test]
fn native_views_and_known_team_scopes_have_required_host_checks() {
    let mut f = Fixture::new();
    let mut host = Host::new();
    host.views = false;
    let e = surfaces::validate(&f.contract(), &host).unwrap_err();
    assert_eq!(e.pointer.as_deref(), Some("/surfaces/0/uiPath"));
    assert_eq!(host.view_calls.get(), 1);
    host.views = true;
    host.scopes = false;
    f.surfaces[0].permission = Some("catalog.read".into());
    let e = surfaces::validate(&f.contract(), &host).unwrap_err();
    assert_eq!(e.pointer.as_deref(), Some("/surfaces/0/permission"));
    host.scopes = true;
    surfaces::validate(&f.contract(), &host).unwrap();
    f.permissions.clear();
    let e = surfaces::validate(&f.contract(), &host).unwrap_err();
    assert_eq!(e.pointer.as_deref(), Some("/permissions"));
}
#[test]
fn service_surfaces_retain_paths_but_storefront_blocks_always_require_native_views() {
    let mut f = Fixture::new();
    f.runtime = "service".into();
    f.surfaces[0].ui_path = "dist/ui.html".into();
    let mut host = Host::new();
    host.views = false;
    surfaces::validate(&f.contract(), &host).unwrap();
    assert_eq!(host.view_calls.get(), 0);
    f.surfaces[0].location = "storefront.block".into();
    f.surfaces[0].actions = vec!["read".into()];
    let e = surfaces::validate(&f.contract(), &host).unwrap_err();
    assert_eq!(e.pointer.as_deref(), Some("/surfaces/0/uiPath"));
}
#[test]
fn placement_paths_locations_ids_and_sizes_stay_closed() {
    for location in [
        "admin.navigation",
        "admin.b2b.company",
        "admin.b2b.unit",
        "account.b2b",
        "admin.pos.catalog",
        "admin.pos.cart",
        "admin.pos.receipt",
        "admin.order",
        "admin.product",
        "admin.product.general",
        "admin.product.tab",
        "admin.customer",
        "admin.order.general",
        "storefront.page",
        "storefront.block",
        "storefront.home",
        "storefront.header",
        "product.detail",
        "cart.summary",
        "checkout.content",
        "checkout.summary",
        "account.overview",
    ] {
        let mut f = Fixture::new();
        f.surfaces[0].location = location.into();
        f.surfaces[0].actions = vec!["read".into()];
        surfaces::validate(&f.contract(), &Host::new()).unwrap();
    }
    for path in [
        "",
        "/root",
        "../secret",
        "native/../secret",
        "native//manager",
        "native/./manager",
        "https://example.test/ui",
        "native/view?token=x",
    ] {
        let mut f = Fixture::new();
        f.surfaces[0].ui_path = path.into();
        assert!(
            surfaces::validate(&f.contract(), &Host::new()).is_err(),
            "{path}"
        );
    }
    for change in 0..4 {
        let mut f = Fixture::new();
        match change {
            0 => f.surfaces[0].location = "unknown".into(),
            1 => f.surfaces[0].id = "Bad ID".into(),
            2 => f.surfaces[0].ui_path = "x".repeat(161),
            _ => f.surfaces[0].actions = vec!["read".into(); 25],
        };
        assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
    }
    let mut f = Fixture::new();
    f.surfaces.push(f.surfaces[0].clone());
    assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
    let mut f = Fixture::new();
    f.surfaces = (0..16)
        .map(|i| {
            let mut s = f.surfaces[0].clone();
            s.id = format!("surface{i}");
            s
        })
        .collect();
    surfaces::validate(&f.contract(), &Host::new()).unwrap();
    let mut extra = f.surfaces[0].clone();
    extra.id = "extra".into();
    f.surfaces.push(extra);
    assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
}
#[test]
fn get_routes_consult_host_effect_rules_instead_of_trusting_action_metadata() {
    let mut f = Fixture::new();
    f.routes = vec![route("admin", "GET", "read")];
    let mut host = Host::new();
    host.read = false;
    assert!(surfaces::validate(&f.contract(), &host).is_err());
    assert_eq!(host.read_calls.get(), 1);
    f.actions[0].read_only = true;
    assert!(surfaces::validate(&f.contract(), &host).is_err());
    host.read = true;
    surfaces::validate(&f.contract(), &host).unwrap();
    f.routes[0].method = "POST".into();
    host.read = false;
    let before = host.read_calls.get();
    surfaces::validate(&f.contract(), &host).unwrap();
    assert_eq!(host.read_calls.get(), before);
}
#[test]
fn routes_cannot_expose_private_writes_publicly_or_reuse_ambiguous_names() {
    let mut f = Fixture::new();
    f.routes = vec![route("storefront", "POST", "write")];
    assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
    f.routes[0].scope = "admin".into();
    surfaces::validate(&f.contract(), &Host::new()).unwrap();
    for change in 0..4 {
        let mut f = Fixture::new();
        let mut r = route("admin", "POST", "write");
        match change {
            0 => r.scope = "system".into(),
            1 => r.method = "DELETE".into(),
            2 => r.path = "../users".into(),
            _ => r.action = "missing".into(),
        };
        f.routes = vec![r];
        assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
    }
    let mut f = Fixture::new();
    f.routes = vec![route("admin", "GET", "read"), route("admin", "GET", "read")];
    assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
    f.routes[1].scope = "storefront".into();
    f.routes.push(route("admin", "POST", "write"));
    surfaces::validate(&f.contract(), &Host::new()).unwrap();
    f.routes = (0..24)
        .map(|i| {
            let mut r = route("admin", "GET", "read");
            r.path = format!("route{i}");
            r
        })
        .collect();
    surfaces::validate(&f.contract(), &Host::new()).unwrap();
    let mut extra = route("admin", "GET", "read");
    extra.path = "extra".into();
    f.routes.push(extra);
    assert!(surfaces::validate(&f.contract(), &Host::new()).is_err());
}
