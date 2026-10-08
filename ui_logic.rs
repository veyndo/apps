//! Declarative UI code-behind validates a bounded AST; calls retain surface grants and server domain authorization.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Expression {
    Literal {
        value: Value,
    },
    Value {
        block: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },
    Record {
        block: String,
    },
    Object {
        fields: HashMap<String, Expression>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Statement {
    Set {
        target: String,
        value: Expression,
    },
    If {
        left: Expression,
        compare: String,
        right: Expression,
        then: Vec<Statement>,
        #[serde(default)]
        otherwise: Vec<Statement>,
    },
    Call {
        action: String,
        input: Expression,
    },
    MsgBox {
        text: HashMap<String, String>,
    },
    Navigate {
        view: String,
    },
    Refresh {
        target: String,
    },
    Validate {
        target: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Handlers {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub click: Vec<Statement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change: Vec<Statement>,
}
pub(super) const CONTROLS: &[&str] = &[
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
pub(super) fn validate(
    m: &Manifest,
    v: &native_views::NativeView,
    b: &native_views::NativeBlock,
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
        statements(m, v, &h.click, 0, &mut count)?;
        statements(m, v, &h.change, 0, &mut count)?;
    }
    Ok(())
}
fn block<'a>(v: &'a native_views::NativeView, id: &str) -> Result<&'a native_views::NativeBlock> {
    v.blocks
        .iter()
        .find(|b| b.id == id)
        .ok_or(bad("Code-behind references an unknown block"))
}
fn expression(
    m: &Manifest,
    v: &native_views::NativeView,
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
            if fields.len() > 32 || fields.keys().any(|n| !identifier(n)) {
                return Err(bad("Invalid expression object"));
            }
            for e in fields.values() {
                expression(m, v, e, depth + 1, count)?;
            }
        }
    };
    Ok(())
}
fn statements(
    m: &Manifest,
    v: &native_views::NativeView,
    items: &[Statement],
    depth: usize,
    count: &mut usize,
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
                ui_logic_types::set(m, v, target, value)?;
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
                statements(m, v, then, depth + 1, count)?;
                statements(m, v, otherwise, depth + 1, count)?;
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
