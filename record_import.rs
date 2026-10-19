//! Private source-record intake reuses an installed app's declared native save action, schema, rights and package fence.
use super::*;
pub(crate) async fn import_record(
    a: &App,
    h: &RequestContext,
    id: &str,
    destination: &Value,
    expected_digest: &str,
) -> Result<Value> {
    let (m, entity_name, input) = admitted(a, h, id, destination, expected_digest).await?;
    let t = merchant(a, h)?;
    let entity = m
        .entities
        .iter()
        .find(|e| e.name == entity_name)
        .ok_or(bad("Destination entity unavailable"))?;
    let mut tx = a.db.begin().await?;
    history::context(&mut tx, h, "migration.record.import").await?;
    let result = data::import_tx(&mut tx, &t, &m, entity, &input).await?;
    tx.commit().await?;
    Ok(json!({"id":result["id"],"replayed":result["replayed"]==true}))
}

/// The same destination package, declared save action and native field normalization govern comparison.
async fn admitted(
    a: &App,
    h: &RequestContext,
    id: &str,
    destination: &Value,
    expected_digest: &str,
) -> Result<(Manifest, String, Value)> {
    auth::permit(h, "apps.manage")?;
    let t = merchant(a, h)?;
    if staging::parent(a, &t).await?.is_none() {
        return Err(bad("Managed record imports require a private environment"));
    }
    let app = destination["app"]
        .as_str()
        .ok_or(bad("Destination app required"))?;
    let action_name = destination["action"]
        .as_str()
        .ok_or(bad("Destination save action required"))?;
    let m = package(a, &t, app, true).await?;
    if expected_digest != approval::canonical_digest(&m) {
        return Err(conflict(
            "Destination app package changed; review the target again",
        ));
    }
    if m.runtime != "declarative" {
        return Err(bad("Import destination must use managed native records"));
    }
    let action = m
        .actions
        .iter()
        .find(|x| x.name == action_name && x.handler == "save" && !x.public)
        .ok_or(bad("Declared private save action required"))?;
    auth::permit(h, action.permission.as_deref().unwrap_or("catalog"))?;
    gateway::additional_permissions(h, action)?;
    let entity = m
        .entities
        .iter()
        .find(|x| Some(&x.name) == action.entity.as_ref())
        .ok_or(bad("Destination entity unavailable"))?;
    core_extensions::access(entity, h, true)?;
    let input = data::normalize_input(
        &m,
        &json!({"id":id,"revision":0,"fields":destination["fields"]}),
    )?;
    gateway::validate_native_input(&action.input_schema, &input)?;
    let name = entity.name.clone();
    Ok((m, name, input))
}
pub(crate) async fn inspect_import_record(
    a: &App,
    h: &RequestContext,
    id: &str,
    destination: &Value,
    expected_digest: &str,
) -> Result<(Value, Option<Value>)> {
    let (m, entity_name, input) = admitted(a, h, id, destination, expected_digest).await?;
    let t = merchant(a, h)?;
    let entity = m
        .entities
        .iter()
        .find(|e| e.name == entity_name)
        .ok_or(bad("Destination entity unavailable"))?;
    core_extensions::access(entity, h, false)?;
    let actual = data::list_page(a, &t, &m, entity, &json!({"id":id,"limit":1})).await?["elements"]
        .as_array()
        .and_then(|xs| xs.first().cloned());
    let expected: serde_json::Map<String, Value> = entity
        .fields
        .iter()
        .map(|f| (f.name.clone(), input["fields"][&f.name].clone()))
        .collect();
    Ok((json!(expected), actual))
}
