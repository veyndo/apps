//! Common package grammar and ordered extension compiler; full host extension, identity and trust owners remain mandatory ports.
use crate::{Action, Entity, Error, Result, Slot, bad, declarations};
use declarations::identifier;
pub struct Configuration<'a> {
    pub entity: &'a str,
    pub price_field: &'a str,
    pub input_field: &'a str,
    pub record: &'a str,
    pub wasm_source: &'a str,
}
/// Borrowed projection from the host's complete typed package, never a replacement for its strict decoder.
pub struct Contract<'a> {
    pub id: &'a str,
    pub version: &'a str,
    pub core_api: &'a str,
    pub runtime: &'a str,
    pub category: Option<&'a str>,
    pub permissions: &'a [String],
    pub entities: &'a [Entity],
    pub actions: &'a [Action],
    pub slots: &'a [Slot],
    pub events: &'a [String],
    pub configuration: Option<Configuration<'a>>,
    pub has_payment_provider: bool,
}
/// These policies come from the trusted host, not package JSON or model output.
pub trait Policy: declarations::ReferencePolicy {
    fn known_scope(&self, scope: &str) -> bool;
    fn component_source(&self, source: &str) -> bool;
}
/// Required surrounding validators consume the host's complete package. There are no default acceptors.
pub trait Extensions {
    type Error;
    fn before(&self) -> std::result::Result<(), Self::Error>;
    fn after(&self) -> std::result::Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum CompileError<E> {
    Host(E),
    Model(Error),
}
pub fn compile<P: Policy, H: Extensions>(
    m: &Contract<'_>,
    policy: &P,
    host: &H,
) -> std::result::Result<(), CompileError<H::Error>> {
    host.before().map_err(CompileError::Host)?;
    validate(m, policy).map_err(CompileError::Model)?;
    host.after().map_err(CompileError::Host)
}
fn validate<P: Policy>(m: &Contract<'_>, policy: &P) -> Result<()> {
    if m.category
        .is_some_and(|c| !["commerce", "payment", "api", "ai", "design", "operations"].contains(&c))
    {
        return Err(bad("Unknown app category"));
    }
    if !identifier(m.id)
        || m.core_api != "1"
        || !["declarative", "service"].contains(&m.runtime)
        || m.version.split('.').count() != 3
        || !m.version.split('.').all(|v| v.parse::<u32>().is_ok())
        || m.entities.len() > 12
        || m.actions.len() > 24
        || m.slots.len() > 12
        || m.events.len() > 12
    {
        return Err(bad(
            "Unsupported app identity, API version, runtime or package limits",
        ));
    }
    if m.permissions.iter().any(|p| {
        ![
            "data.read",
            "data.write",
            "storefront.slot",
            "admin.slot",
            "service.call",
            "events.read",
            "events.publish",
            "events.send",
            "knowledge.write",
            "payments.provider",
            "orders.read",
            "orders.import",
            "payments.record",
            "payments.instructions.read",
            "channels.read",
            "documents.read",
            "documents.create",
            "deliveries.write",
            "b2b.read",
            "b2b.manage",
            "customers.read",
            "customers.write",
            "settings.read",
            "products.read",
            "products.write",
            "reviews.write",
            "promotions.write",
            "jobs.read",
            "jobs.write",
            "commerce.hooks",
            "commerce.lifecycle",
            "assets.read",
            "assets.write",
        ]
        .contains(&p.as_str())
            && !p.strip_prefix("events:").is_some_and(|v| {
                v == "self"
                    || (!v.is_empty()
                        && v.len() <= 100
                        && v.bytes().all(|b| {
                            b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'*'
                        }))
            })
            && p != "customers.pii"
    }) {
        return Err(bad("Unknown app permission"));
    }
    if m.entities
        .iter()
        .flat_map(|e| &e.fields)
        .filter(|f| f.indexed || f.unique)
        .count()
        > 64
    {
        return Err(bad("Maximum 64 declared indexes per app package"));
    }
    declarations::validate(m.entities, policy)?;
    if let Some(c) = &m.configuration
        && (!identifier(c.entity)
            || !identifier(c.price_field)
            || !identifier(c.input_field)
            || c.record.is_empty()
            || c.record.len() > 100
            || c.wasm_source.len() > 32768
            || policy.component_source(c.wasm_source)
            || !m.permissions.contains(&"data.read".into())
            || !m.permissions.contains(&"data.write".into())
            || !m.entities.iter().any(|e| {
                e.name == c.entity
                    && e.fields
                        .iter()
                        .any(|f| f.name == c.price_field && f.kind == "integer" && f.required)
            }))
    {
        return Err(bad("Invalid app configuration contract"));
    }
    crate::package_actions::validate(m, policy)?;
    for s in m.slots {
        if !["product.detail", "admin.apps", "admin.order"].contains(&s.location.as_str())
            || ![
                "entity-form",
                "entity-list",
                "engraving",
                "product-configuration",
                "payments",
                "iframe",
            ]
            .contains(&s.component.as_str())
        {
            return Err(bad(
                "Unsupported UI slot; executable JavaScript is not accepted",
            ));
        }
    }
    if m.events.iter().any(|event| {
        event.len() > 100
            || !event
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'*')
    }) {
        return Err(bad("Invalid app event name"));
    }
    if !m.events.is_empty() && m.runtime != "service" {
        return Err(bad("Event subscriptions require service runtime"));
    }
    Ok(())
}
