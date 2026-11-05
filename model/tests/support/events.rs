//! Test-only admitted package projection shared by independent event privacy and delivery cases.
use veyndo_app_model::events::{Context, EventFilter};
pub struct Fixture {
    pub events: Vec<String>,
    pub permissions: Vec<String>,
    pub filters: Vec<EventFilter>,
}
impl Fixture {
    pub fn new(events: &[&str], permissions: &[&str]) -> Self {
        Self {
            events: events.iter().map(|s| s.to_string()).collect(),
            permissions: permissions.iter().map(|s| s.to_string()).collect(),
            filters: vec![],
        }
    }
    pub fn context(&self) -> Context<'_> {
        Context {
            id: "advice",
            events: &self.events,
            permissions: &self.permissions,
            event_filters: &self.filters,
        }
    }
}
