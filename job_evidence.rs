//! Native job completion and authenticated private record evidence share one transaction and current creator/package fences.
use super::*;
fn binding(t: &str, m: &Manifest, entity: &str, id: &str) -> String {
    format!(
        "job-evidence-v1:{t}:{}:{entity}:{id}:{}",
        m.id,
        approval::canonical_digest(m)
    )
}
fn document(e: &Entity, v: &Value) -> Value {
    e.fields
        .iter()
        .filter(|f| f.name != "evidence")
        .map(|f| (f.name.clone(), v[&f.name].clone()))
        .collect()
}
pub(super) async fn save(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    h: &RequestContext,
    identity: &RequestContext,
    m: &Manifest,
    job: &sqlx::postgres::PgRow,
    v: &Value,
) -> Result<()> {
    let Some(evidence) = v.get("evidence") else {
        return Ok(());
    };
    if h.principal.app.as_deref() != Some(m.id.as_str()) || v["status"] != "succeeded" {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Only the approved job service can seal completion evidence".into(),
        ));
    }
    for p in ["jobs.write", "data.write"] {
        if !m.permissions.iter().any(|s| s == p) {
            return Err(bad("Job evidence capability not declared"));
        }
        credentials::permit(h, p)?;
        auth::permit(identity, credentials::scope(p).unwrap())?;
    }
    input_schema::validate_native(
        &json!({"type":"object","properties":{"action":{"type":"string","maxLength":32},"input":{"type":"object"}},"required":["action","input"],"additionalProperties":false}),
        evidence,
    )?;
    let action = m
        .actions
        .iter()
        .find(|a| a.name == evidence["action"] && a.handler == "save" && !a.public)
        .ok_or(bad("Private evidence save action required"))?;
    auth::permit(
        identity,
        action.permission.as_deref().unwrap_or("apps.manage"),
    )?;
    gateway::additional_permissions(identity, action)?;
    let entity = m
        .entities
        .iter()
        .find(|e| Some(&e.name) == action.entity.as_ref() && !e.public_read)
        .ok_or(bad("Private evidence entity required"))?;
    core_extensions::access(entity, identity, true)?;
    let frozen: Value = job.get("input");
    let mut input = data::normalize_input(m, &evidence["input"])?;
    let id = input["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 100)
        .ok_or(bad("Evidence record identity required"))?
        .to_owned();
    if frozen["evidenceRecord"] != json!({"entity":entity.name,"id":id}) {
        return Err(conflict(
            "Evidence destination differs from the reviewed job",
        ));
    }
    if input["fields"]["result"] != v["result"] {
        return Err(conflict("Evidence result differs from the completed job"));
    }
    let fields = input["fields"]
        .as_object_mut()
        .ok_or(bad("Evidence fields required"))?;
    fields.remove("evidence");
    let proof = json!({"v":1,"job":v["id"],"actor":job.get::<String,_>("actor"),"action":job.get::<String,_>("action"),"inputHash":hash(&frozen.to_string()),"fieldsHash":hash(&document(entity,&input["fields"]).to_string()),"at":chrono::Utc::now().to_rfc3339()});
    input["fields"]["evidence"] = json!(
        veyndo::inference::settings::seal(&binding(t, m, &entity.name, &id), &proof.to_string())
            .map_err(|_| Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Job evidence encryption unavailable".into()
            ))?
    );
    gateway::validate_native_input(&action.input_schema, &input)?;
    history::context(tx, identity, "app.job.completed").await?;
    data::save_tx(tx, t, m, entity, &input).await?;
    Ok(())
}
/// Verify actual current records, never caller-provided fields or booleans. Ordinary edits invalidate their evidence.
pub(super) async fn verify(a: &App, h: &RequestContext, m: &Manifest, v: &Value) -> Result<Value> {
    input_schema::validate(
        &json!({"type":"object","properties":{"entity":{"type":"string","maxLength":32},"ids":{"type":"array","maxItems":100,"items":{"type":"string","maxLength":100}}},"required":["entity","ids"],"additionalProperties":false}),
        v,
    )?;
    credentials::permit(h, "data.read")?;
    if !m.permissions.iter().any(|s| s == "data.read") {
        return Err(bad("Evidence read capability not declared"));
    }
    let t = merchant(a, h)?;
    let e = m
        .entities
        .iter()
        .find(|e| e.name == v["entity"] && !e.public_read)
        .ok_or(bad("Private evidence entity required"))?;
    core_extensions::access(e, h, false)?;
    let mut tx = a.db.begin().await?;
    lock_package_write(&mut tx, &t, m).await?;
    let mut verified = vec![];
    for id in v["ids"].as_array().unwrap() {
        let id = id.as_str().unwrap();
        let Some(row) = records::get(&mut tx, &t, &m.id, &e.name, id, false).await? else {
            continue;
        };
        let Some(token) = row["evidence"].as_str().filter(|s| s.len() <= 2000) else {
            continue;
        };
        let Ok(raw) = veyndo::inference::settings::open(&binding(&t, m, &e.name, id), token) else {
            continue;
        };
        let Ok(proof) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if proof["v"] != 1 || proof["fieldsHash"] != hash(&document(e, &row).to_string()) {
            continue;
        }
        let Some(action) = m
            .actions
            .iter()
            .find(|a| a.name == proof["action"] && a.handler == "job")
        else {
            continue;
        };
        auth::permit(h, action.permission.as_deref().unwrap_or("apps.manage"))?;
        gateway::additional_permissions(h, action)?;
        verified.push(json!({"id":id,"revision":row["revision"],"job":proof["job"]}));
    }
    tx.commit().await?;
    Ok(json!({"verified":verified}))
}
