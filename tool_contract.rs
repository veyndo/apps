//! Installed app tools share native wire metadata and validate declared results without disclosing provider payloads.
use super::*;
pub(crate) async fn app_tools(a: &App, h: &RequestContext) -> Result<Vec<Value>> {
    let rows =
        sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active ORDER BY id")
            .bind(tenant(h)?)
            .fetch_all(&a.db)
            .await?;
    let mut tools = vec![];
    for r in rows {
        let m: Manifest =
            serde_json::from_value(r.get("manifest")).map_err(|_| bad("Invalid package"))?;
        for action in &m.actions {
            if verified_kernel::app_tool_admissible(
                action.mcp != Some(false),
                super::gateway::action_authorized(a, h, action),
            ) {
                tools.push(capabilities::ToolContract::describe(
                    &format!("app.{}.{}", m.id, action.name),
                    &action.description,
                    &action.input_schema,
                    action.output_schema.as_ref(),
                    action.read_only
                        || ["list", "configurations"].contains(&action.handler.as_str()),
                ));
            }
        }
    }
    Ok(tools)
}
pub(super) fn checked_output(action: &Action, result: Value) -> Result<Value> {
    if let Some(schema) = &action.output_schema {
        input_schema::validate_value(schema, &result).map_err(|_| Error::output_contract())?;
    }
    Ok(result)
}
