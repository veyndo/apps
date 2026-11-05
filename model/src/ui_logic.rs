//! UI behavior validation preserves bounded ASTs, typed controls and every surface action allowlist.
use crate::ui_ast::{Expression, Statement};
use crate::{
    Result, bad, ui_logic_types,
    views::{Contract, NativeBlock, NativeView, Policy},
};
pub(crate) const CONTROLS: &[&str] = &[
    "button",
    "textbox",
    "combobox",
    "checkbox",
    "datepicker",
    "image",
    "frame",
    "tabs",
    "kpi",
    "chart",
];
pub(crate) fn validate<P: Policy>(
    m: &Contract<'_>,
    v: &NativeView,
    b: &NativeBlock,
    policy: &P,
) -> Result<()> {
    if let Some(field) = &b.data_field {
        let e = m
            .entities
            .iter()
            .find(|e| Some(&e.name) == b.entity.as_ref())
            .ok_or(bad("Control data model required"))?;
        let f = e
            .fields
            .iter()
            .find(|f| &f.name == field)
            .ok_or(bad("Unknown control field"))?;
        let valid = match b.kind.as_str() {
            "textbox" => ["string", "integer", "decimal"].contains(&f.kind.as_str()),
            "checkbox" => f.kind == "boolean",
            "datepicker" => ["date", "datetime"].contains(&f.kind.as_str()),
            "combobox" => !f.choices.is_empty() || f.references.is_some(),
            "image" => f.kind == "image",
            "kpi" | "chart" => ["integer", "decimal", "money"].contains(&f.kind.as_str()),
            _ => false,
        };
        if !valid {
            return Err(bad("Control does not support this field type"));
        }
    } else if [
        "textbox",
        "combobox",
        "checkbox",
        "datepicker",
        "image",
        "kpi",
        "chart",
    ]
    .contains(&b.kind.as_str())
    {
        return Err(bad("Control dataField required"));
    }
    if b.child_blocks.len() > 32
        || b.child_blocks
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != b.child_blocks.len()
        || b.child_blocks.iter().any(|id| {
            v.blocks
                .iter()
                .filter(|parent| parent.child_blocks.contains(id))
                .count()
                > 1
        })
        || b.child_blocks.iter().any(|id| {
            id == &b.id
                || !v.blocks.iter().any(|b| &b.id == id)
                || v.blocks
                    .iter()
                    .any(|b| &b.id == id && !b.child_blocks.is_empty())
        })
        || !b.child_blocks.is_empty() && !["frame", "tabs"].contains(&b.kind.as_str())
    {
        return Err(bad("Containers support one level of existing leaf blocks"));
    }
    if let Some(h) = &b.handlers {
        if !h.click.is_empty() && b.kind != "button"
            || !h.change.is_empty()
                && !["textbox", "combobox", "checkbox", "datepicker"].contains(&b.kind.as_str())
        {
            return Err(bad("Control event is not supported"));
        }
        let mut count = 0;
        statements(m, v, &h.click, 0, &mut count, policy)?;
        statements(m, v, &h.change, 0, &mut count, policy)?;
    }
    Ok(())
}
fn block<'a>(v: &'a NativeView, id: &str) -> Result<&'a NativeBlock> {
    v.blocks
        .iter()
        .find(|b| b.id == id)
        .ok_or(bad("Code-behind references an unknown block"))
}
fn expression(
    m: &Contract<'_>,
    v: &NativeView,
    e: &Expression,
    depth: usize,
    count: &mut usize,
) -> Result<()> {
    *count += 1;
    if depth > 8 || *count > 256 {
        return Err(bad("Code-behind exceeds depth/node limit"));
    }
    match e {
        Expression::Literal { value } => {
            if value.to_string().len() > 8192 {
                return Err(bad("Code literal exceeds 8 KiB"));
            }
        }
        Expression::Value { block: id, field } => {
            let b = block(v, id)?;
            if let Some(name) = field {
                if b.kind != "form" {
                    return Err(bad("Field expressions require a form record"));
                }
                let entity = m
                    .entities
                    .iter()
                    .find(|e| Some(&e.name) == b.entity.as_ref())
                    .ok_or(bad("Block data model required"))?;
                if !entity.fields.iter().any(|f| &f.name == name) {
                    return Err(bad("Unknown record field"));
                }
            } else if !["textbox", "combobox", "checkbox", "datepicker"].contains(&b.kind.as_str())
            {
                return Err(bad("Block has no scalar value"));
            }
        }
        Expression::Record { block: id } => {
            if block(v, id)?.kind != "form" {
                return Err(bad("Record expression needs a form"));
            }
        }
        Expression::Object { fields } => {
            if fields.len() > 32 || fields.keys().any(|n| !object_key(n)) {
                return Err(bad("Invalid expression object"));
            }
            for e in fields.values() {
                expression(m, v, e, depth + 1, count)?;
            }
        }
    };
    Ok(())
}
/// API input objects use camelCase as well as schema identifiers; prototype and unbounded keys remain prohibited.
fn object_key(name: &str) -> bool {
    !["__proto__", "constructor", "prototype"].contains(&name)
        && !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn statements<P: Policy>(
    m: &Contract<'_>,
    v: &NativeView,
    items: &[Statement],
    depth: usize,
    count: &mut usize,
    policy: &P,
) -> Result<()> {
    if depth > 8 {
        return Err(bad("Code-behind nesting exceeds eight"));
    }
    for s in items {
        *count += 1;
        if *count > 256 {
            return Err(bad("Code-behind exceeds 256 nodes"));
        }
        match s {
            Statement::Set { target, value } => {
                if !["textbox", "combobox", "checkbox", "datepicker"]
                    .contains(&block(v, target)?.kind.as_str())
                {
                    return Err(bad("Set needs an input control"));
                }
                expression(m, v, value, depth + 1, count)?;
                ui_logic_types::set(m, v, target, value, policy)?;
            }
            Statement::If {
                left,
                compare,
                right,
                then,
                otherwise,
            } => {
                if !["eq", "ne", "gt", "ge", "lt", "le"].contains(&compare.as_str()) {
                    return Err(bad("Invalid comparison"));
                }
                expression(m, v, left, depth + 1, count)?;
                expression(m, v, right, depth + 1, count)?;
                ui_logic_types::comparison(m, v, left, right, compare)?;
                statements(m, v, then, depth + 1, count, policy)?;
                statements(m, v, otherwise, depth + 1, count, policy)?;
            }
            Statement::Call { action, input } => {
                if !m.actions.iter().any(|a| &a.name == action) {
                    return Err(bad("Unknown code-behind action"));
                }
                for s in m
                    .surfaces
                    .iter()
                    .filter(|s| s.ui_path == format!("native/{}", v.id))
                {
                    if !s.actions.contains(action) {
                        return Err(bad("Code action is missing from surface allowlist"));
                    }
                }
                expression(m, v, input, depth + 1, count)?;
                ui_logic_types::call(m, v, action, input)?;
            }
            Statement::MsgBox { text } => {
                if text.is_empty() || text.len() > 100 || text.values().any(|s| s.len() > 1000) {
                    return Err(bad("Localized message required"));
                }
            }
            Statement::Navigate { view } => {
                if !m
                    .surfaces
                    .iter()
                    .any(|s| s.ui_path == format!("native/{view}"))
                {
                    return Err(bad("Navigation target is not exposed"));
                }
            }
            Statement::Refresh { target } => {
                let b = block(v, target)?;
                if b.read_action.is_none() {
                    return Err(bad("Refresh requires a data-bound block"));
                }
            }
            Statement::Validate { target } => {
                if block(v, target)?.kind != "form" {
                    return Err(bad("Validate requires a form"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "ui_object_key_tests.rs"]
mod object_key_tests;
