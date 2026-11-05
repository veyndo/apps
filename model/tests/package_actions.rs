//! Cross-feature action scenarios retain private/public, job, payment, asset and configurator contracts in the independent compiler.
#[path = "support/package.rs"]
mod fixture;
use fixture::*;
use serde_json::json;
#[test]
fn public_tools_cannot_read_private_entities_or_declare_team_rights() {
    let mut m = base();
    m["entities"] = json!([entity(false)]);
    let mut a = action("list");
    a["entity"] = json!("records");
    m["actions"] = json!([a]);
    compile(&m).unwrap();
    m["actions"][0]["public"] = json!(true);
    rejected(&m, "Private entity cannot be a public tool");
    m["entities"][0]["publicRead"] = json!(true);
    compile(&m).unwrap();
    for (key, value) in [
        ("permission", json!("orders.read")),
        ("requiredPermissions", json!(["orders.read"])),
    ] {
        let mut x = m.clone();
        x["actions"][0][key] = value;
        rejected(&x, "Invalid app action");
    }
    let mut x = m.clone();
    x["actions"][0]["entity"] = json!("missing");
    rejected(&x, "Invalid app action");
    for handler in ["list", "save"] {
        let mut x = base();
        x["actions"] = json!([action(handler)]);
        rejected(&x, "Entity action requires entity");
    }
}
#[test]
fn public_submit_is_append_only_private_and_never_a_flow_action() {
    let mut m = base();
    m["permissions"] = json!(["data.write"]);
    m["entities"] = json!([entity(false)]);
    let mut a = action("submit");
    a["entity"] = json!("records");
    a["public"] = json!(true);
    m["actions"] = json!([a]);
    compile(&m).unwrap();
    for (key, value) in [
        ("public", json!(false)),
        ("readOnly", json!(true)),
        ("flowAllowed", json!(true)),
    ] {
        let mut x = m.clone();
        x["actions"][0][key] = value;
        rejected(&x, "Submit requires an append-only private entity");
    }
    let mut x = m.clone();
    x["entities"][0]["publicRead"] = json!(true);
    rejected(&x, "Submit requires an append-only private entity");
    m["permissions"] = json!([]);
    rejected(&m, "Submit requires an append-only private entity");
}
#[test]
fn leased_jobs_need_service_runtime_write_capability_and_own_completion_event() {
    let mut m = base();
    m["runtime"] = json!("service");
    m["permissions"] = json!(["jobs.write"]);
    m["events"] = json!(["app.example.job_execute"]);
    m["actions"] = json!([action("job")]);
    compile(&m).unwrap();
    for (key, value) in [("public", json!(true)), ("readOnly", json!(true))] {
        let mut x = m.clone();
        x["actions"][0][key] = value;
        rejected(&x, "Invalid app action");
    }
    for (key, value) in [
        ("permissions", json!([])),
        ("events", json!(["app.foreign.job_execute"])),
        ("runtime", json!("declarative")),
    ] {
        let mut x = m.clone();
        x[key] = value;
        rejected(&x, "Invalid app action");
    }
    m["events"] = json!(["app.example.job_queued"]);
    compile(&m).unwrap();
}
#[test]
fn payment_actions_require_provider_and_private_management_scope() {
    for handler in ["payment_command", "payment_onboarding"] {
        let mut m = base();
        m["paymentProvider"] = json!({"fixture":"host extension validates the complete contract"});
        let mut a = action(handler);
        a["permission"] = json!("payments.manage");
        m["actions"] = json!([a]);
        compile(&m).unwrap();
        for (key, value) in [
            ("public", json!(true)),
            ("readOnly", json!(true)),
            ("permission", json!("orders.read")),
        ] {
            let mut x = m.clone();
            x["actions"][0][key] = value;
            rejected(&x, "Invalid app action");
        }
        let mut x = m.clone();
        x.as_object_mut().unwrap().remove("paymentProvider");
        rejected(&x, "Invalid app action");
        if handler == "payment_onboarding" {
            m["actions"][0]["flowAllowed"] = json!(true);
            rejected(&m, "Invalid app action");
        }
    }
}
#[test]
fn asset_reads_and_uploads_have_distinct_private_read_write_requirements() {
    for handler in ["assets", "asset_preview", "asset_upload"] {
        let mut m = base();
        m["permissions"] = json!(["assets.read", "assets.write"]);
        let mut a = action(handler);
        a["readOnly"] = json!(handler != "asset_upload");
        m["actions"] = json!([a]);
        compile(&m).unwrap();
        let mut x = m.clone();
        x["actions"][0]["public"] = json!(true);
        rejected(&x, "Invalid app action");
        let mut x = m.clone();
        x["actions"][0]["readOnly"] = json!(handler == "asset_upload");
        rejected(&x, "Invalid app action");
        let mut x = m.clone();
        x["permissions"] = json!([]);
        rejected(&x, "Invalid app action");
        if handler == "asset_upload" {
            m["permissions"] = json!(["assets.read"]);
            rejected(&m, "Invalid app action");
        }
    }
}
#[test]
fn configuration_and_events_keep_their_specific_flow_and_effect_contracts() {
    let mut m = configured();
    let mut a = action("configure");
    a["public"] = json!(true);
    m["actions"] = json!([a]);
    compile(&m).unwrap();
    for (key, value) in [
        ("public", json!(false)),
        ("readOnly", json!(true)),
        ("flowAllowed", json!(true)),
    ] {
        let mut x = m.clone();
        x["actions"][0][key] = value;
        rejected(&x, "Invalid app action");
    }
    m.as_object_mut().unwrap().remove("configuration");
    rejected(&m, "Invalid app action");
    for handler in ["save", "configurations"] {
        let mut x = base();
        let mut a = action(handler);
        a["public"] = json!(true);
        x["actions"] = json!([a]);
        rejected(&x, "Invalid app action");
    }
    let mut m = base();
    m["permissions"] = json!(["events.publish"]);
    m["actions"] = json!([action("emit")]);
    compile(&m).unwrap();
    for (key, value) in [("public", json!(true)), ("readOnly", json!(true))] {
        let mut x = m.clone();
        x["actions"][0][key] = value;
        rejected(&x, "Invalid app action");
    }
    m["permissions"] = json!([]);
    rejected(&m, "Invalid app action");
}
