//! Package capability, schema and action validation; no executable behavior is inferred from names.
use super::*;
pub(crate) fn validate(m: &Manifest) -> Result<()> {
    if m.category
        .as_deref()
        .is_some_and(|c| !["commerce", "payment", "api", "ai", "design", "operations"].contains(&c))
    {
        return Err(bad("Unknown app category"));
    }
    if !identifier(&m.id)
        || m.core_api != "1"
        || !["declarative", "service"].contains(&m.runtime.as_str())
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
            "knowledge.write",
        ]
        .contains(&p.as_str())
    }) {
        return Err(bad("Unknown app permission"));
    }
    let mut names = std::collections::HashSet::new();
    for e in &m.entities {
        if !identifier(&e.name)
            || !names.insert(&e.name)
            || e.fields.is_empty()
            || e.fields.len() > 16
        {
            return Err(bad("Invalid entity"));
        }
        let mut fields = std::collections::HashSet::new();
        for f in &e.fields {
            if !identifier(&f.name)
                || ["tenant", "id", "revision"].contains(&f.name.as_str())
                || !fields.insert(&f.name)
                || !["string", "integer", "boolean"].contains(&f.kind.as_str())
                || (f.translatable && (f.kind != "string" || f.references.is_some()))
                || f.references
                    .as_ref()
                    .is_some_and(|r| f.kind != "string" || !m.entities.iter().any(|e| e.name == *r))
            {
                return Err(bad("Invalid field or entity relationship"));
            }
        }
    }
    if let Some(c) = &m.configuration
        && (!identifier(&c.entity)
            || !identifier(&c.price_field)
            || !identifier(&c.input_field)
            || c.record.is_empty()
            || c.record.len() > 100
            || c.wasm_source.len() > 32768
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
    let mut actions = std::collections::HashSet::new();
    for a in &m.actions {
        if !identifier(&a.name)
            || !actions.insert(&a.name)
            || a.description.len() > 300
            || !["list", "save", "service", "configurations", "emit"].contains(&a.handler.as_str())
            || a.permission
                .as_deref()
                .is_some_and(|p| !auth::SCOPES.contains(&p))
            || (a.handler == "emit" && !m.permissions.contains(&"events.publish".into()))
            || (a.handler == "emit" && a.public)
            || (a.handler == "emit" && a.read_only)
            || a.input_schema["type"] != "object"
            || a.input_schema["additionalProperties"] != false
            || (a.handler == "save" && a.public)
            || (a.handler == "configurations" && a.public)
            || a.entity
                .as_ref()
                .is_some_and(|n| !m.entities.iter().any(|e| e.name == *n))
        {
            return Err(bad("Invalid app action"));
        }
        if ["list", "save"].contains(&a.handler.as_str()) && a.entity.is_none() {
            return Err(bad("Entity action requires entity"));
        }
        if a.public
            && a.entity
                .as_ref()
                .is_some_and(|n| !m.entities.iter().any(|e| e.name == *n && e.public_read))
        {
            return Err(bad("Private entity cannot be a public tool"));
        }
    }
    for s in &m.slots {
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
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
    }) {
        return Err(bad("Invalid app event name"));
    }
    if !m.events.is_empty()
        && (!m.permissions.contains(&"events.read".into()) || m.runtime != "service")
    {
        return Err(bad(
            "Event subscriptions require service runtime and events.read",
        ));
    }
    Ok(())
}
