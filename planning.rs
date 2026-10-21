//! Registered managed app actions join the same preview/approve transaction as core changes.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AppChange {
    pub app: String,
    pub action: String,
    pub arguments_json: String,
    #[serde(default)]
    pub expected_app_revision: i64,
}
pub(crate) async fn planning_context(
    a: &App,
    t: &str,
    h: &RequestContext,
    query: &str,
) -> Result<Value> {
    let rows =
        sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active ORDER BY id")
            .bind(t)
            .fetch_all(&a.db)
            .await?;
    let mut manifests = rows
        .into_iter()
        .map(|r| {
            serde_json::from_value::<Manifest>(r.get("manifest"))
                .map_err(|_| bad("Invalid app package"))
        })
        .collect::<Result<Vec<_>>>()?;
    manifests.retain(|m| {
        m.actions.iter().any(|act| {
            gateway::action_authorized(a, h, act)
                && act.mcp != Some(false)
                && m.intelligence
                    .as_ref()
                    .is_none_or(|ai| ai.tools.contains(&act.name))
        })
    });
    manifests.sort_by_key(|m| {
        (
            std::cmp::Reverse(discovery::relevant(m, query)),
            m.id.clone(),
        )
    });
    let mut context = vec![];
    for m in manifests.into_iter().take(8) {
        let mut records = vec![];
        if m.permissions.contains(&"data.read".into()) {
            for e in m
                .entities
                .iter()
                .filter(|e| {
                    m.actions.iter().any(|act| {
                        act.handler == "list"
                            && act.mcp != Some(false)
                            && act.entity.as_deref() == Some(e.name.as_str())
                            && gateway::action_authorized(a, h, act)
                            && m.intelligence
                                .as_ref()
                                .is_none_or(|ai| ai.tools.contains(&act.name))
                    }) && m
                        .intelligence
                        .as_ref()
                        .is_none_or(|ai| ai.entities.contains(&e.name))
                })
                .take(4)
            {
                let reader = m
                    .actions
                    .iter()
                    .find(|act| {
                        act.handler == "list"
                            && act.mcp != Some(false)
                            && act.entity.as_deref() == Some(e.name.as_str())
                            && gateway::action_authorized(a, h, act)
                            && m.intelligence
                                .as_ref()
                                .is_none_or(|ai| ai.tools.contains(&act.name))
                    })
                    .ok_or(bad("No authorized app reader"))?;
                let mut data = data::list_scoped(
                    a,
                    t,
                    &m,
                    e,
                    &json!({"limit":12}),
                    reader.public.then(|| marketing::channel_id(h)),
                )
                .await?;
                data["elements"] = json!(
                    data["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .take(12)
                        .collect::<Vec<_>>()
                );
                if let Some(rows) = data["elements"].as_array_mut() {
                    for row in rows {
                        *row = bounded_context(row, 0);
                    }
                }
                records.push(json!({"entity":e.name,"records":data}));
            }
        }
        let actions = m
            .actions
            .iter()
            .filter(|act| {
                gateway::action_authorized(a, h, act)
                    && act.mcp != Some(false)
                    && m.intelligence
                        .as_ref()
                        .is_none_or(|ai| ai.tools.contains(&act.name))
            })
            .collect::<Vec<_>>();
        let mut intelligence = m.intelligence.clone();
        if let Some(ai) = &mut intelligence {
            ai.tools
                .retain(|name| actions.iter().any(|act| act.name == *name));
            ai.entities
                .retain(|name| records.iter().any(|record| record["entity"] == *name));
            ai.ontology
                .retain(|node| ai.entities.contains(&node.entity));
        }
        let item =
            json!({"app":m.id,"actions":actions,"records":records,"intelligence":intelligence});
        if context
            .iter()
            .map(|v: &Value| v.to_string().len())
            .sum::<usize>()
            + item.to_string().len()
            > 32768
        {
            break;
        }
        context.push(item);
    }
    Ok(json!(context))
}
pub(crate) async fn bind_change(
    a: &App,
    t: &str,
    c: &mut AppChange,
    h: &RequestContext,
) -> Result<()> {
    if c.arguments_json.len() > 32768 {
        return Err(bad("App change too large"));
    }
    let row = sqlx::query(
        "SELECT manifest,revision FROM app_packages WHERE tenant=$1 AND id=$2 AND active",
    )
    .bind(t)
    .bind(&c.app)
    .fetch_optional(&a.db)
    .await?
    .ok_or(bad("Unknown active app in proposal"))?;
    let m: Manifest =
        serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid app package"))?;
    let action = m
        .actions
        .iter()
        .find(|a| a.name == c.action && a.handler == "save")
        .ok_or(bad("Planner supports managed save actions only"))?;
    authorize_planned_action(&m, action, h)?;
    let e = m
        .entities
        .iter()
        .find(|e| Some(&e.name) == action.entity.as_ref())
        .ok_or(bad("Unknown app entity"))?;
    let mut args: Value =
        serde_json::from_str(&c.arguments_json).map_err(|_| bad("Invalid app arguments JSON"))?;
    data::fields(e, &args["fields"])?;
    if m.id == "site_builder" {
        site_builder::validate(&e.name, &args["fields"])?;
    }
    let id = args["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 100)
        .ok_or(bad("App record ID required"))?;
    let revision = data::record_revision(a, t, &m, e, id).await?;
    args["revision"] = json!(revision);
    c.arguments_json = args.to_string();
    c.expected_app_revision = row.get("revision");
    Ok(())
}
pub(crate) async fn apply_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    c: &AppChange,
    h: &RequestContext,
) -> Result<()> {
    let row = sqlx::query(
        "SELECT manifest,revision FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(t)
    .bind(&c.app)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(conflict("App is not active"))?;
    if row.get::<i64, _>("revision") != c.expected_app_revision {
        return Err(conflict("App changed since proposal"));
    }
    let m: Manifest =
        serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid app package"))?;
    let action = m
        .actions
        .iter()
        .find(|a| a.name == c.action && a.handler == "save")
        .ok_or(bad("Unknown managed app action"))?;
    authorize_planned_action(&m, action, h)?;
    let e = m
        .entities
        .iter()
        .find(|e| Some(&e.name) == action.entity.as_ref())
        .ok_or(bad("Unknown app entity"))?;
    let args: Value =
        serde_json::from_str(&c.arguments_json).map_err(|_| bad("Invalid app arguments"))?;
    data::save_tx(tx, t, &m, e, &args).await?;
    Ok(())
}

/// App descriptions/records are untrusted data; bound nested JSON before entering a model prompt.
fn bounded_context(v: &Value, depth: usize) -> Value {
    if depth >= 4 {
        return json!("[context depth limit]");
    }
    match v {
        Value::String(s) => json!(s.chars().take(300).collect::<String>()),
        Value::Array(items) => json!(
            items
                .iter()
                .take(8)
                .map(|v| bounded_context(v, depth + 1))
                .collect::<Vec<_>>()
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .take(16)
                .map(|(k, v)| (k.clone(), bounded_context(v, depth + 1)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// A proposal is untrusted model output; both selection and current action rights are checked before binding and committing.
pub(super) fn authorize_planned_action(
    m: &Manifest,
    action: &Action,
    h: &RequestContext,
) -> Result<()> {
    if action.mcp == Some(false)
        || m.intelligence
            .as_ref()
            .is_some_and(|ai| !ai.tools.contains(&action.name))
    {
        return Err(bad("App action is not selected for merchant planning"));
    }
    gateway::additional_permissions(h, action)?;
    auth::permit(h, action.permission.as_deref().unwrap_or("catalog"))
}
