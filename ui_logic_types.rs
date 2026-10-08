//! Static UI expression types prevent control coercion and invoking save with a different model; runtime still validates actual values.
use super::native_views::{NativeBlock, NativeView};
use super::ui_logic::Expression;
use super::*;
#[derive(PartialEq)]
enum Type {
    String,
    Integer,
    Boolean,
    Decimal,
    Object,
    Array,
    Null,
}
fn field_type(f: &Field) -> Type {
    if f.kind == "relations" {
        return Type::Array;
    }
    if f.translatable || field_values::json(f) {
        return Type::Object;
    }
    match f.kind.as_str() {
        "integer" => Type::Integer,
        "boolean" => Type::Boolean,
        "decimal" => Type::Decimal,
        _ => Type::String,
    }
}
fn find<'a>(v: &'a NativeView, id: &str) -> Result<&'a NativeBlock> {
    v.blocks
        .iter()
        .find(|b| b.id == id)
        .ok_or(bad("Unknown expression block"))
}
fn field<'a>(m: &'a Manifest, b: &NativeBlock, name: &str) -> Result<&'a Field> {
    m.entities
        .iter()
        .find(|e| Some(&e.name) == b.entity.as_ref())
        .and_then(|e| e.fields.iter().find(|f| f.name == name))
        .ok_or(bad("Unknown expression field"))
}
fn infer(m: &Manifest, v: &NativeView, e: &Expression) -> Result<Type> {
    Ok(match e {
        Expression::Literal { value } => match value {
            Value::String(_) => Type::String,
            Value::Bool(_) => Type::Boolean,
            Value::Number(n) if n.is_i64() => Type::Integer,
            Value::Object(_) => Type::Object,
            Value::Array(_) => Type::Array,
            Value::Null => Type::Null,
            _ => return Err(bad("UI numeric literals must be exact integers")),
        },
        Expression::Value { block, field: name } => {
            let b = find(v, block)?;
            field_type(field(
                m,
                b,
                name.as_deref()
                    .or(b.data_field.as_deref())
                    .ok_or(bad("Control field required"))?,
            )?)
        }
        Expression::Object { .. } | Expression::Record { .. } => Type::Object,
    })
}
pub(super) fn set(m: &Manifest, v: &NativeView, target: &str, e: &Expression) -> Result<()> {
    let b = find(v, target)?;
    let f = field(
        m,
        b,
        b.data_field
            .as_deref()
            .ok_or(bad("Input control needs a field"))?,
    )?;
    if let Expression::Literal { value } = e {
        let mut optional = f.clone();
        optional.required = false;
        let entity = Entity {
            name: "literal".into(),
            label: HashMap::new(),
            public_read: false,
            fields: vec![optional],
        };
        let mut fields = serde_json::Map::new();
        fields.insert(f.name.clone(), value.clone());
        return field_values::validate(&entity, &Value::Object(fields));
    }
    if infer(m, v, e)? != field_type(f) {
        return Err(bad("Set expression type differs from its control field"));
    }
    Ok(())
}
pub(super) fn comparison(
    m: &Manifest,
    v: &NativeView,
    left: &Expression,
    right: &Expression,
    compare: &str,
) -> Result<()> {
    let l = infer(m, v, left)?;
    let r = infer(m, v, right)?;
    if !["eq", "ne"].contains(&compare) && (l != r || !matches!(l, Type::String | Type::Integer)) {
        return Err(bad(
            "Ordered comparison requires matching text or integer expressions",
        ));
    }
    if l != r && l != Type::Null && r != Type::Null {
        return Err(bad("Comparison expression types differ"));
    }
    Ok(())
}
pub(super) fn call(m: &Manifest, v: &NativeView, name: &str, e: &Expression) -> Result<()> {
    let action = m
        .actions
        .iter()
        .find(|a| a.name == name)
        .ok_or(bad("Unknown action"))?;
    if infer(m, v, e)? != Type::Object {
        return Err(bad("Action input must be an object"));
    }
    if let Expression::Literal { value } = e {
        input_schema::validate(&action.input_schema, value)?;
    }
    if let Expression::Record { block } = e
        && action.handler == "save"
        && find(v, block)?.entity != action.entity
    {
        return Err(bad("Save action targets a different form model"));
    }
    Ok(())
}
