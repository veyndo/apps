//! Bounded UTC cron and private incoming webhook declarations; clock and durable execution belong to the host.
use crate::{Action, Result, bad, declarations::identifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSchedule {
    pub id: String,
    pub cron: String,
    pub action: String,
    pub input: Value,
    pub enabled: bool,
}
pub fn parse(expression: &str) -> Result<cron::Schedule> {
    // Six fields including a single seconds value: at most one tick per minute per schedule.
    let parts = expression.split_whitespace().collect::<Vec<_>>();
    if expression.len() > 100 || parts.len() != 6 || parts[0].parse::<u8>().is_err() {
        return Err(bad(
            "Cron requires six UTC fields with a single seconds value",
        ));
    }
    cron::Schedule::from_str(expression).map_err(|_| bad("Invalid cron expression"))
}
pub fn schedules(
    schedules: &[AppSchedule],
    actions: &[Action],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if schedules.len() > 8 {
        return Err(bad("Maximum eight app schedules"));
    }
    for s in schedules {
        let action = actions
            .iter()
            .find(|a| a.name == s.action && a.handler == "emit")
            .ok_or(bad("Schedule requires a declared emit action"))?;
        if !identifier(&s.id)
            || !ids.insert(&s.id)
            || s.input.to_string().len() > 16384
            || parse(&s.cron)?.after(&now).next().is_none()
        {
            return Err(bad("Invalid or exhausted app schedule"));
        }
        crate::schema::validate(&action.input_schema, &s.input)?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppWebhook {
    pub id: String,
    pub action: String,
}
pub fn webhooks(webhooks: &[AppWebhook], actions: &[Action]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if webhooks.len() > 8 {
        return Err(bad("Maximum eight app webhooks"));
    }
    for w in webhooks {
        if !identifier(&w.id)
            || !ids.insert(&w.id)
            || !actions
                .iter()
                .any(|a| a.name == w.action && a.handler == "emit" && !a.public)
        {
            return Err(bad("Webhook requires a private emit action"));
        }
    }
    Ok(())
}
