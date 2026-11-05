//! Execute privacy projections and deterministic schedule validation without Core, SQL or external services.
use serde_json::json;
use veyndo_app_model::{
    Action,
    events::{self, Context, DestinationPolicy},
    subscriptions,
    triggers::{self, AppSchedule},
};
struct NoExternalDelivery;
impl DestinationPolicy for NoExternalDelivery {
    type Error = &'static str;
    fn validate_destination(&self, _: &str) -> Result<(), Self::Error> {
        Err("No external destination configured in this example")
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let declared = vec!["order.placed".into()];
    let permissions = vec!["events:order.placed".into()];
    let context = Context {
        id: "advice",
        events: &declared,
        permissions: &permissions,
        event_filters: &[],
    };
    subscriptions::validate(&context)?;
    assert!(events::validate(&context, None, &NoExternalDelivery).is_ok());
    let event = json!({"orderId":"example","currency":"EUR","totalMinor":1250,"email":"synthetic-private@example.test"});
    assert!(subscriptions::eligible(&context, "order.placed", &event));
    println!(
        "App-visible event: {}",
        subscriptions::payload(&context, "order.placed", event)
    );
    let action: Action = serde_json::from_value(
        json!({"name":"tick","description":"Tick","handler":"emit","public":false,"mcp":false,"inputSchema":{"type":"object","additionalProperties":false}}),
    )?;
    let schedule: AppSchedule = serde_json::from_value(
        json!({"id":"daily","cron":"0 */15 * * * *","action":"tick","input":{},"enabled":true}),
    )?;
    let now =
        chrono::DateTime::parse_from_rfc3339("2026-10-05T10:01:00Z")?.with_timezone(&chrono::Utc);
    triggers::schedules(std::slice::from_ref(&schedule), &[action], now)?;
    println!(
        "Next UTC tick: {}",
        triggers::parse(&schedule.cron)?.after(&now).next().unwrap()
    );
    Ok(())
}
