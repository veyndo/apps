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
pub(crate) async fn planning_context(a: &App, t: &str) -> Result<Value> {
    let rows = sqlx::query(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND active ORDER BY id LIMIT 8",
    )
    .bind(t)
    .fetch_all(&a.db)
    .await?;
    let mut context = vec![];
    for row in rows {
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid app package"))?;
        let mut records = vec![];
        if m.permissions.contains(&"data.read".into()) {
            for e in m
                .entities
                .iter()
                .filter(|e| {
                    m.intelligence
                        .as_ref()
                        .is_none_or(|ai| ai.entities.contains(&e.name))
                })
                .take(4)
            {
                let mut data = data::list_page(a, t, &m, e, &json!({"limit":12})).await?;
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
                m.intelligence
                    .as_ref()
                    .is_none_or(|ai| ai.tools.contains(&act.name))
            })
            .collect::<Vec<_>>();
        let item =
            json!({"app":m.id,"actions":actions,"records":records,"intelligence":m.intelligence});
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
pub(crate) async fn bind_change(a: &App, t: &str, c: &mut AppChange) -> Result<()> {
    if c.arguments_json.len() > 2000 {
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
    let e = m
        .entities
        .iter()
        .find(|e| Some(&e.name) == action.entity.as_ref())
        .ok_or(bad("Unknown app entity"))?;
    let mut args: Value =
        serde_json::from_str(&c.arguments_json).map_err(|_| bad("Invalid app arguments JSON"))?;
    data::fields(e, &args["fields"])?;
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
