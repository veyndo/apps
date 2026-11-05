//! Shared actor, compound permission and schema admission for native app actions and pinned callbacks.
use super::*;
pub(super) fn check(a: &App, h: &RequestContext, action: &Action, v: &Value) -> Result<()> {
    if !action.public {
        merchant(a, h)?;
    }
    if !action.public
        && (["save", "service", "emit", "job"].contains(&action.handler.as_str())
            || action.permission.is_some())
    {
        auth::permit(
            h,
            action
                .permission
                .as_deref()
                .unwrap_or(if action.handler == "job" {
                    "apps.manage"
                } else {
                    "catalog"
                }),
        )?;
    }
    gateway::additional_permissions(h, action)?;
    gateway::validate_input(&action.input_schema, v)?;
    Ok(())
}
