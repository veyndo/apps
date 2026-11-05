//! Object binding scenarios preserve native object rights, PII requirements and private unique anchors.
use serde_json::json;
use veyndo_app_model::{Action, Entity, Object, object_extensions};
fn model(object: Object) -> Entity {
    serde_json::from_value(json!({"name":"notes","extends":{"version":1,"object":object,"field":"owner"},"fields":[{"name":"owner","kind":"string","required":true,"indexed":true,"unique":true,"coreReference":object.reference()}]})).unwrap()
}
fn actions(object: Object) -> Vec<Action> {
    serde_json::from_value(json!([
 {"name":"read","description":"Read","handler":"list","entity":"notes","inputSchema":{"type":"object"},"requiredPermissions":object.scopes(false)},
 {"name":"write","description":"Write","handler":"save","entity":"notes","inputSchema":{"type":"object"},"requiredPermissions":object.scopes(true)}
])).unwrap()
}
fn permissions() -> Vec<String> {
    vec!["data.read".into(), "data.write".into()]
}
#[test]
fn product_customer_order_company_bindings_keep_exact_scope_sets() {
    for object in [
        Object::Product,
        Object::Customer,
        Object::Order,
        Object::Company,
    ] {
        let e = model(object);
        let a = actions(object);
        object_extensions::validate(std::slice::from_ref(&e), &a, &permissions()).unwrap();
        for index in 0..a.len() {
            for right in object.scopes(index == 1) {
                let mut missing = a.clone();
                missing[index].required_permissions.retain(|s| s != right);
                assert!(
                    object_extensions::validate(std::slice::from_ref(&e), &missing, &permissions())
                        .is_err(),
                    "{right}"
                );
            }
        }
        let mut primary = a.clone();
        for a in &mut primary {
            a.permission = Some(a.required_permissions.remove(0));
        }
        object_extensions::validate(&[e], &primary, &permissions()).unwrap();
    }
}
#[test]
fn bindings_cannot_make_personal_or_operational_records_public() {
    for object in [
        Object::Product,
        Object::Customer,
        Object::Order,
        Object::Company,
    ] {
        for change in 0..9 {
            let mut e = model(object);
            match change {
                0 => e.public_read = true,
                1 => e.fields[0].required = false,
                2 => e.fields[0].indexed = false,
                3 => e.fields[0].unique = false,
                4 => e.fields[0].kind = "integer".into(),
                5 => e.fields[0].translatable = true,
                6 => e.fields[0].core_reference = Some("wrong".into()),
                7 => e.fields[0].references = Some("notes".into()),
                _ => e.extends.as_mut().unwrap().version = 2,
            };
            assert!(object_extensions::validate(&[e], &actions(object), &permissions()).is_err());
        }
    }
    let mut e = model(Object::Product);
    e.extends.as_mut().unwrap().field = "missing".into();
    assert!(object_extensions::validate(&[e], &actions(Object::Product), &permissions()).is_err());
    let mut e = model(Object::Product);
    e.publication = Some(
        serde_json::from_value(
            json!({"statusField":"state","channelField":"channel","startsAt":"starts"}),
        )
        .unwrap(),
    );
    assert!(object_extensions::validate(&[e], &actions(Object::Product), &permissions()).is_err());
}
#[test]
fn extension_actions_require_list_read_permission_and_private_native_handlers() {
    let e = model(Object::Customer);
    let a = actions(Object::Customer);
    assert!(object_extensions::validate(std::slice::from_ref(&e), &a, &[]).is_err());
    assert!(
        object_extensions::validate(std::slice::from_ref(&e), &a[1..], &permissions()).is_err()
    );
    for (public, handler) in [
        (true, "list"),
        (false, "service"),
        (false, "emit"),
        (true, "save"),
    ] {
        let mut bad = a.clone();
        bad[0].public = public;
        bad[0].handler = handler.into();
        assert!(
            object_extensions::validate(std::slice::from_ref(&e), &bad, &permissions()).is_err()
        );
    }
    let mut unbound = e;
    unbound.extends = None;
    object_extensions::validate(&[unbound], &[], &[]).unwrap();
}
#[test]
fn model_wire_keeps_absence_and_closed_binding_object_names() {
    let mut e = model(Object::Product);
    e.extends = None;
    let value = serde_json::to_value(e).unwrap();
    assert!(value.get("extends").is_none());
    assert!(value.get("operational").is_none());
    for raw in [
        json!({"version":1,"object":"sql","field":"owner"}),
        json!({"version":1,"object":"product","field":"owner","table":"products"}),
    ] {
        let mut value = serde_json::to_value(model(Object::Product)).unwrap();
        value["extends"] = raw;
        assert!(serde_json::from_value::<Entity>(value).is_err());
    }
}
