//! Optional server-enforced publication window and channel projection for public app records.
use super::*;
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Publication {
    pub status_field: String,
    pub channel_field: String,
    pub starts_at: String,
}
pub(super) fn validate(e: &Entity) -> Result<()> {
    let Some(p) = &e.publication else {
        return Ok(());
    };
    for (name, kind) in [
        (&p.status_field, "string"),
        (&p.channel_field, "string"),
        (&p.starts_at, "datetime"),
    ] {
        if !e.public_read
            || !e.fields.iter().any(|f| {
                f.name == *name
                    && f.kind == kind
                    && !f.translatable
                    && (kind == "datetime" || f.indexed)
            })
        {
            return Err(bad(
                "Publication needs public scalar status/channel indexes and a datetime",
            ));
        }
    }
    if p.status_field == p.channel_field {
        return Err(bad("Publication fields must be distinct"));
    }
    Ok(())
}
pub(super) fn predicate(query: &mut sqlx::QueryBuilder<sqlx::Postgres>, e: &Entity, channel: &str) {
    if let Some(p) = &e.publication {
        query
            .push(format!(
                " AND {}='published' AND ({}='*' OR {}=",
                column(&p.status_field),
                column(&p.channel_field),
                column(&p.channel_field)
            ))
            .push_bind(channel)
            .push(format!(
                ") AND ({} IS NULL OR {}::timestamptz<=now())",
                column(&p.starts_at),
                column(&p.starts_at)
            ));
    }
}
