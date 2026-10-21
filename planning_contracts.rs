//! Current authorized managed-save models accompany proposals; no records, secrets or new authority.
use super::*;
pub(crate) async fn contracts(
    a: &App,
    t: &str,
    h: &RequestContext,
    ids: &[String],
) -> Result<Value> {
    if ids.is_empty() {
        return Ok(json!([]));
    }
    auth::permit(h, "apps.read")?;
    if ids.len() > 12 || ids.iter().any(|id| !identifier(id)) {
        return Err(bad("App contract selection exceeds the planning limit"));
    }
    let rows = sqlx::query("SELECT manifest,revision FROM app_packages WHERE tenant=$1 AND id=ANY($2) AND active ORDER BY id")
        .bind(t).bind(ids).fetch_all(&a.db).await?;
    let mut result = Vec::new();
    for row in rows {
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid app package"))?;
        for action in m.actions.iter().filter(|action| action.handler == "save") {
            if planning::authorize_planned_action(&m, action, h).is_err() {
                continue;
            }
            let e = m
                .entities
                .iter()
                .find(|e| Some(&e.name) == action.entity.as_ref())
                .ok_or(bad("Unknown app entity"))?;
            if core_extensions::access(e, h, true).is_err() {
                continue;
            }
            let entry = json!({"app":m.id,"version":m.version,"revision":row.get::<i64,_>("revision"),"action":action.name,"inputSchema":action.input_schema,"entity":e.name,"fields":e.fields,"nestedFormat":if m.id=="site_builder" {site_builder::planning_contract()} else {Value::Null}});
            if json!(result).to_string().len() + entry.to_string().len() > 32768 {
                return Err(bad(
                    "App contracts exceed the planning context; select fewer apps",
                ));
            }
            result.push(entry);
        }
    }
    Ok(json!(result))
}
