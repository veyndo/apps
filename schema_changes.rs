//! Explicit, transactional field evolution. No raw migration SQL; bounded recovery snapshots precede DDL and every converted value is validated.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Migration {
    pub from_version: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Step {
    Rename {
        entity: String,
        field: String,
        to: String,
    },
    Remove {
        entity: String,
        field: String,
    },
    Convert {
        entity: String,
        field: String,
    },
    Fill {
        entity: String,
        field: String,
        value: Value,
    },
}
impl Step {
    pub(super) fn keys(&self) -> (&str, &str) {
        match self {
            Self::Rename { entity, field, .. }
            | Self::Remove { entity, field }
            | Self::Convert { entity, field }
            | Self::Fill { entity, field, .. } => (entity, field),
        }
    }
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    if m.schema_migrations.len() > 16 {
        return Err(bad("Too many schema migration paths"));
    }
    let mut seen = std::collections::HashSet::new();
    for path in &m.schema_migrations {
        if path.steps.is_empty()
            || path.steps.len() > 32
            || path.from_version == m.version
            || !seen.insert(&path.from_version)
        {
            return Err(bad("Invalid schema migration path"));
        }
        for step in &path.steps {
            let (entity, field) = step.keys();
            if !identifier(entity)
                || !identifier(field)
                || ["tenant", "id", "revision"].contains(&field)
            {
                return Err(bad("Invalid migration field"));
            }
            if let Step::Rename { to, .. } = step
                && (!identifier(to) || ["tenant", "id", "revision"].contains(&to.as_str()))
            {
                return Err(bad("Invalid renamed field"));
            }
            if let Step::Fill { value, .. } = step
                && value.to_string().len() > 32768
            {
                return Err(bad("Migration default exceeds limit"));
            }
        }
    }
    Ok(())
}
fn contract(a: &Field, b: &Field) -> bool {
    a.kind == b.kind
        && a.required == b.required
        && a.translatable == b.translatable
        && a.references == b.references
        && a.core_reference == b.core_reference
        && a.choices
            .iter()
            .all(|v| b.choices.iter().any(|n| n.value == v.value))
        && (!a.choices.is_empty() || b.choices.is_empty())
}
pub(super) fn convert(v: &Value, f: &Field) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let out = match f.kind.as_str() {
        "integer" if v.is_string() => json!(
            v.as_str()
                .unwrap()
                .parse::<i64>()
                .map_err(|_| bad("Migration integer conversion failed"))?
        ),
        "string" | "decimal" if v.is_i64() => json!(v.as_i64().unwrap().to_string()),
        "boolean" if v == "true" => json!(true),
        "boolean" if v == "false" => json!(false),
        _ => v.clone(),
    };
    Ok(out)
}

pub(super) fn shape(old: &Manifest, m: &Manifest, path: Option<&Migration>) -> Result<Vec<Entity>> {
    let mut shape = old.entities.clone();
    if let Some(path) = path {
        for step in &path.steps {
            let (entity, field) = step.keys();
            let e = shape
                .iter_mut()
                .find(|e| e.name == entity)
                .ok_or(bad("Migration source model is missing"))?;
            let next = m
                .entities
                .iter()
                .find(|e| e.name == entity)
                .ok_or(bad("Removing models needs an offline migration"))?;
            let existing = e.fields.iter().find(|f| f.name == field).cloned();
            if existing.as_ref().is_some_and(|f| {
                f.references.is_some()
                    || f.core_reference.is_some()
                    || matches!(f.kind.as_str(), "image" | "file" | "relations")
            }) {
                return Err(bad("Relationship changes need an offline migration"));
            }
            match step {
                Step::Rename { to, .. } => {
                    if e.fields.iter().any(|f| f.name == *to) {
                        return Err(bad("Migration rename target already exists"));
                    }
                    let f = e
                        .fields
                        .iter_mut()
                        .find(|f| f.name == field)
                        .ok_or(bad("Migration source field is missing"))?;
                    f.name = to.clone();
                }
                Step::Remove { .. } => {
                    if existing.is_none() {
                        return Err(bad("Migration source field is missing"));
                    }
                    e.fields.retain(|f| f.name != field);
                }
                Step::Convert { .. } | Step::Fill { .. } => {
                    let f = next
                        .fields
                        .iter()
                        .find(|f| f.name == field)
                        .ok_or(bad("Migration target field is missing"))?;
                    if f.references.is_some()
                        || f.core_reference.is_some()
                        || matches!(f.kind.as_str(), "image" | "file" | "relations")
                    {
                        return Err(bad("Relationship migration is not supported"));
                    }
                    if matches!(step, Step::Fill { .. })
                        && existing.as_ref().is_some_and(|old| {
                            old.kind != f.kind || old.translatable != f.translatable
                        })
                    {
                        return Err(bad("Use convert before changing a filled field type"));
                    }
                    if let Some(i) = e.fields.iter().position(|v| v.name == field) {
                        e.fields[i] = f.clone();
                    } else if matches!(step, Step::Fill { .. }) {
                        e.fields.push(f.clone());
                    } else {
                        return Err(bad("Migration source field is missing"));
                    }
                }
            }
        }
    }
    for e in &shape {
        let next = m
            .entities
            .iter()
            .find(|v| v.name == e.name)
            .ok_or(conflict("Removing models requires an offline migration"))?;
        for f in &e.fields {
            if !next
                .fields
                .iter()
                .any(|v| v.name == f.name && contract(f, v))
            {
                return Err(conflict(
                    "Changing/removing fields requires an explicit schema migration",
                ));
            }
        }
    }
    Ok(shape)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_is_explicit_and_never_float_money() {
        let f: Field = serde_json::from_value(json!({"name":"quantity","kind":"integer"})).unwrap();
        assert_eq!(convert(&json!("42"), &f).unwrap(), json!(42));
        assert!(convert(&json!("2.5"), &f).is_err());
    }
}
