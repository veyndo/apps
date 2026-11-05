//! Entity/field/index/reference declaration grammar; native reference authority is an explicit required policy port.
use crate::{Entity, Result, bad, fields, publication};
pub trait ReferencePolicy {
    fn admitted(&self, product: bool, private: bool) -> bool;
}
pub fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        && s.as_bytes()[0].is_ascii_lowercase()
}
pub fn validate_fields<P: ReferencePolicy>(e: &Entity, policy: &P) -> Result<()> {
    for f in &e.fields {
        if let Some(kind) = f.core_reference.as_deref()
            && (!matches!(
                kind,
                "product" | "customer" | "order" | "b2bCompany" | "b2bUnit" | "salesChannel"
            ) || f.kind != "string"
                || !f.indexed
                || !f.required
                || f.translatable
                || f.references.is_some()
                || !f.choices.is_empty()
                || !policy.admitted(kind == "product", !e.public_read))
        {
            return Err(bad(
                "Core references need indexed private strings; only products may be public",
            ));
        }
        let mut choices = std::collections::HashSet::new();
        if f.choices.len() > 100
            || (!f.choices.is_empty()
                && (f.kind != "string" || f.translatable || f.references.is_some()))
        {
            return Err(bad("Choices require a plain string field, maximum 100"));
        }
        for c in &f.choices {
            if !identifier(&c.value)
                || !choices.insert(&c.value)
                || c.label.is_empty()
                || c.label.len() > 100
                || c.label
                    .values()
                    .any(|v| v.trim().is_empty() || v.len() > 100)
            {
                return Err(bad(
                    "Choice values must be unique identifiers with translated labels",
                ));
            }
        }
    }
    Ok(())
}
pub fn validate<P: ReferencePolicy>(entities: &[Entity], policy: &P) -> Result<()> {
    if entities.len() > 12 {
        return Err(bad("Maximum 12 models per app package"));
    }
    if entities
        .iter()
        .flat_map(|e| &e.fields)
        .filter(|f| f.indexed || f.unique)
        .count()
        > 64
    {
        return Err(bad("Maximum 64 declared indexes per app package"));
    }
    let mut names = std::collections::HashSet::new();
    for e in entities {
        publication::validate(e)?;
        if !identifier(&e.name)
            || !names.insert(&e.name)
            || e.fields.is_empty()
            || e.fields.len() > 16
        {
            return Err(bad("Invalid entity"));
        }
        let mut fields = std::collections::HashSet::new();
        validate_fields(e, policy)?;
        fields::contract(e)?;
        for f in &e.fields {
            if (f.kind == "relations"
                && (f.references.is_none()
                    || f.translatable
                    || f.indexed
                    || f.unique
                    || f.core_reference.is_some()
                    || !f.choices.is_empty()))
                || !identifier(&f.name)
                || ["tenant", "id", "revision"].contains(&f.name.as_str())
                || !fields.insert(&f.name)
                || !fields::KINDS.contains(&f.kind.as_str())
                || (fields::json(f)
                    && (f.indexed || f.unique || (f.references.is_some() && f.kind != "relations")))
                || (f.translatable
                    && (f.kind != "string" || (f.references.is_some() && f.kind != "relations")))
                || f.references.as_ref().is_some_and(|r| {
                    !matches!(f.kind.as_str(), "string" | "relations")
                        || !entities.iter().any(|target| {
                            target.name == *r && (!e.public_read || target.public_read)
                        })
                })
            {
                return Err(bad("Invalid field or entity relationship"));
            }
        }
    }
    Ok(())
}
