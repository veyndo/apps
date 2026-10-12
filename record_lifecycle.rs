//! Generic app-owned record lifecycle contracts. Status vocabulary and transitions belong to the app Manifest.
use super::*;
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Workflow {
    pub field: String,
    pub initial: String,
    pub transitions: Vec<Transition>,
    #[serde(default)]
    pub immutable_fields: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Transition {
    pub from: String,
    pub to: String,
}
pub(super) fn contract(e: &Entity) -> Result<()> {
    let Some(w) = &e.workflow else {
        return Ok(());
    };
    let field = e
        .fields
        .iter()
        .find(|f| f.name == w.field)
        .ok_or(bad("Workflow field is missing"))?;
    let choices = field
        .choices
        .iter()
        .map(|c| c.value.as_str())
        .collect::<Vec<_>>();
    if field.kind != "string"
        || field.translatable
        || !field.required
        || choices.is_empty()
        || choices.len() > 16
        || !choices.contains(&w.initial.as_str())
        || w.transitions.len() > 32
        || w.immutable_fields.len() > 16
        || w.transitions.iter().any(|t| {
            t.from == t.to
                || !choices.contains(&t.from.as_str())
                || !choices.contains(&t.to.as_str())
        })
        || w.immutable_fields
            .iter()
            .any(|name| name == &w.field || !e.fields.iter().any(|f| &f.name == name))
    {
        return Err(bad(
            "Workflow needs a required untranslated choice field, valid transitions and declared immutable fields",
        ));
    }
    Ok(())
}
pub(super) fn change(e: &Entity, old: Option<&Value>, fields: &Value) -> Result<()> {
    let Some(w) = &e.workflow else {
        return Ok(());
    };
    let to = fields[&w.field]
        .as_str()
        .ok_or(bad("Workflow status is required"))?;
    let Some(old) = old else {
        return if to == w.initial {
            Ok(())
        } else {
            Err(bad(format!("New records must start at {}", w.initial)))
        };
    };
    if w.immutable_fields
        .iter()
        .any(|name| old[name] != fields[name])
    {
        return Err(conflict(
            "Workflow record identity fields cannot be changed",
        ));
    }
    let from = old[&w.field]
        .as_str()
        .ok_or(conflict("Stored workflow status is missing"))?;
    if from != to && !w.transitions.iter().any(|t| t.from == from && t.to == to) {
        return Err(conflict(&format!(
            "Workflow transition {from} → {to} is not allowed"
        )));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entity() -> Entity {
        serde_json::from_value(json!({"name":"cases","fields":[{"name":"status","kind":"string","required":true,"choices":[{"value":"new","label":{}},{"value":"closed","label":{}}]},{"name":"order_id","kind":"string"}],"workflow":{"field":"status","initial":"new","immutableFields":["order_id"],"transitions":[{"from":"new","to":"closed"}]}})).unwrap()
    }
    #[test]
    fn lifecycle_rejects_skipped_creation_reverse_states_and_identity_swaps() {
        let e = entity();
        contract(&e).unwrap();
        let new = json!({"status":"new","order_id":"owned"});
        let closed = json!({"status":"closed","order_id":"owned"});
        assert!(change(&e, None, &new).is_ok());
        assert!(change(&e, None, &closed).is_err());
        assert!(change(&e, Some(&new), &closed).is_ok());
        assert!(change(&e, Some(&closed), &closed).is_ok());
        assert!(change(&e, Some(&closed), &new).is_err());
        assert!(
            change(
                &e,
                Some(&new),
                &json!({"status":"new","order_id":"different"})
            )
            .is_err()
        );
    }
    #[test]
    fn invalid_choice_and_identity_contracts_are_rejected() {
        let mut e = entity();
        e.workflow.as_mut().unwrap().initial = "unknown".into();
        assert!(contract(&e).is_err());
        e = entity();
        e.workflow
            .as_mut()
            .unwrap()
            .immutable_fields
            .push("missing".into());
        assert!(contract(&e).is_err());
        e = entity();
        e.fields[0].translatable = true;
        assert!(contract(&e).is_err());
    }
}
