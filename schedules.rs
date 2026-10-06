//! Durable UTC cron ticks emit namespaced outbox events; replicas lock due rows and staging never runs them.
use super::*;
use std::str::FromStr;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppSchedule {
    pub id: String,
    pub cron: String,
    pub action: String,
    pub input: Value,
    pub enabled: bool,
}
fn parse(expression: &str) -> Result<cron::Schedule> {
    // Six fields including a single seconds value: at most one tick per minute per schedule.
    let parts = expression.split_whitespace().collect::<Vec<_>>();
    if expression.len() > 100 || parts.len() != 6 || parts[0].parse::<u8>().is_err() {
        return Err(bad(
            "Cron requires six UTC fields with a single seconds value",
        ));
    }
    cron::Schedule::from_str(expression).map_err(|_| bad("Invalid cron expression"))
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if m.schedules.len() > 8 {
        return Err(bad("Maximum eight app schedules"));
    }
    for s in &m.schedules {
        let action = m
            .actions
            .iter()
            .find(|a| a.name == s.action && a.handler == "emit")
            .ok_or(bad("Schedule requires a declared emit action"))?;
        if !identifier(&s.id)
            || !ids.insert(&s.id)
            || s.input.to_string().len() > 16384
            || parse(&s.cron)?.upcoming(chrono::Utc).next().is_none()
        {
            return Err(bad("Invalid or exhausted app schedule"));
        }
        validate_input(&action.input_schema, &s.input)?;
    }
    Ok(())
}
pub(super) async fn sync(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
) -> Result<()> {
    let ids = m.schedules.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
    sqlx::query("DELETE FROM app_schedules WHERE tenant=$1 AND app=$2 AND NOT(id=ANY($3))")
        .bind(t)
        .bind(&m.id)
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    for s in &m.schedules {
        let next = parse(&s.cron)?
            .upcoming(chrono::Utc)
            .next()
            .ok_or(bad("Cron has no future occurrence"))?
            .to_rfc3339();
        // Identical installs retain their due time. Changing a schedule starts its new clock.
        sqlx::query("INSERT INTO app_schedules(tenant,app,id,definition,next_run) VALUES($1,$2,$3,$4,$5::text::timestamptz) ON CONFLICT(tenant,app,id) DO UPDATE SET next_run=CASE WHEN app_schedules.definition=$4 THEN app_schedules.next_run ELSE EXCLUDED.next_run END,definition=$4")
            .bind(t).bind(&m.id).bind(&s.id).bind(json!(s)).bind(next).execute(&mut **tx).await?;
    }
    Ok(())
}
pub(crate) async fn schedule_once(a: &App) -> Result<()> {
    let mut tx = a.db.begin().await?;
    // Lock the package as well: deactivation/release cannot race an old schedule into the live outbox.
    let rows=sqlx::query("SELECT s.tenant,s.app,s.id,s.definition,s.next_run::text AS due FROM app_schedules s JOIN app_packages p ON p.tenant=s.tenant AND p.id=s.app WHERE p.active AND EXISTS(SELECT 1 FROM tenants t WHERE t.id=coalesce((SELECT live_tenant FROM shop_environments WHERE tenant=s.tenant),s.tenant) AND t.status='active') AND NOT EXISTS(SELECT 1 FROM shop_environments e WHERE e.tenant=s.tenant) AND s.next_run<=now() AND s.definition->>'enabled'='true' ORDER BY s.next_run LIMIT 32 FOR UPDATE OF s,p SKIP LOCKED").fetch_all(&mut *tx).await?;
    for r in rows {
        let t = r.get::<String, _>("tenant");
        let app = r.get::<String, _>("app");
        let id = r.get::<String, _>("id");
        let s: AppSchedule = serde_json::from_value(r.get("definition"))
            .map_err(|_| bad("Invalid stored schedule"))?;
        let next = parse(&s.cron)?
            .upcoming(chrono::Utc)
            .next()
            .ok_or(bad("Schedule exhausted"))?
            .to_rfc3339();
        let event: i64 = sqlx::query_scalar(
            "INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3) RETURNING id",
        )
        .bind(&t)
        .bind(format!("app.{app}.{}", s.action))
        .bind(&s.input)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO app_schedule_runs(tenant,app,schedule,due,event_id) VALUES($1,$2,$3,$4::text::timestamptz,$5)").bind(&t).bind(&app).bind(&id).bind(r.get::<String,_>("due")).bind(event).execute(&mut *tx).await?;
        sqlx::query("UPDATE app_schedules SET next_run=$1::text::timestamptz WHERE tenant=$2 AND app=$3 AND id=$4").bind(next).bind(t).bind(app).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cron_is_bounded_and_real() {
        for c in ["* * * * * *", "0 * * * *", "61 * * * * *", "0 99 * * * *"] {
            assert!(parse(c).is_err());
        }
        let s = parse("0 */15 * * * *").unwrap();
        let start = chrono::DateTime::parse_from_rfc3339("2026-10-05T10:01:00Z").unwrap();
        assert_eq!(
            s.after(&start).next().unwrap().to_rfc3339(),
            "2026-10-05T10:15:00+00:00"
        );
    }
}
