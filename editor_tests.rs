//! Assistant fixture contracts exercise real validation, not only the client-side builder.
use super::*;
fn example(name: &str) -> Manifest {
    serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/extensions/apps/assistant-examples/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn every_assistant_builds_a_valid_normal_package() {
    for name in [
        "frontend",
        "admin",
        "combined",
        "payment",
        "shipping",
        "integration",
        "event",
        "webhook",
        "scheduled",
        "customer",
        "order",
        "product_general",
    ] {
        validate(&example(name)).unwrap_or_else(|e| panic!("{name}: {}", e.1));
    }
}
#[test]
fn editor_context_and_private_core_references_cannot_be_smuggled() {
    for mutation in 0..7 {
        let mut m = example("customer");
        match mutation {
            0 => m.entities[0].public_read = true,
            1 => m.entities[0].fields[0].indexed = false,
            2 => m.entities[0].fields[0].core_reference = Some("arbitrary_table".into()),
            3 => m.views[0].blocks[0].context_binding.as_mut().unwrap().key = "orderId".into(),
            4 => m.views[0].blocks[0].context_binding.as_mut().unwrap().field = "title".into(),
            5 => m.entities[0].fields[0].required = false,
            _ => m.surfaces[0].location = "admin.navigation".into(),
        }
        assert!(validate(&m).is_err(), "mutation {mutation}");
    }
}
#[test]
fn choice_values_are_typed_and_checked_at_write_time() {
    let m = example("admin");
    let e = &m.entities[0];
    let mut v = json!({"product_id":"mug","title":{"en":"Care"},"segment":"standard"});
    assert!(data::fields(e, &v).is_ok());
    v["segment"] = json!("unlisted");
    assert!(data::fields(e, &v).is_err());
    v["segment"] = json!(true);
    assert!(data::fields(e, &v).is_err());
    let mut m = m.clone();
    let c = m.entities[0].fields[3].choices[0].clone();
    m.entities[0].fields[3].choices.push(c);
    assert!(validate(&m).is_err());
}
#[test]
fn automation_cannot_invoke_an_arbitrary_mutation() {
    let mut m = example("scheduled");
    m.schedules[0].action = "save_entries".into();
    assert!(validate(&m).is_err());
    let mut m = example("webhook");
    m.webhooks[0].action = "list_entries".into();
    assert!(validate(&m).is_err());
    let mut m = example("payment");
    m.actions[0].public = true;
    assert!(validate(&m).is_err());
}
