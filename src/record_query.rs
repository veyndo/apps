//! Original bounded native page/filter admission and declared-field projection; caller owns current authority and transaction.
use crate::model::Entity;
use crate::{
    Error,
    queries::{Filter, Query, Window},
};
use serde_json::{Value, json};
fn invalid(message: &'static str) -> Error {
    Error::Invalid(message)
}
pub struct Plan<'a> {
    pub limit: i64,
    pub after: &'a str,
    pub id: Option<&'a str>,
    pub filters: Vec<Filter<'a>>,
    pub publication: Option<Window<'a>>,
}
impl<'a> Plan<'a> {
    pub fn query(&'a self) -> Query<'a> {
        Query {
            limit: self.limit,
            after: self.after,
            id: self.id,
            filters: &self.filters,
            publication: self.publication.as_ref().map(|p| Window {
                status_field: p.status_field,
                channel_field: p.channel_field,
                starts_at: p.starts_at,
                channel: p.channel,
            }),
        }
    }
}
pub fn prepare<'a>(
    permissions: &[String],
    entity: &'a Entity,
    input: &'a Value,
    channel: Option<&'a str>,
) -> Result<Plan<'a>, Error> {
    if !permissions.contains(&"data.read".into()) {
        return Err(Error::Invalid("App needs data.read"));
    }
    let limit = input["limit"].as_i64().unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(invalid("App page limit must be 1..100"));
    }
    let after = input["after"].as_str().unwrap_or("");
    if after.len() > 100 {
        return Err(invalid("App cursor exceeds limit"));
    }
    let id = input
        .get("id")
        .map(|id| {
            id.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 100)
                .ok_or(invalid("Invalid app record ID"))
        })
        .transpose()?;
    let mut filters = Vec::new();
    if let Some(filter) = input.get("filter") {
        let filter = filter
            .as_object()
            .ok_or(invalid("App filter must be an object"))?;
        if filter.len() > 4 {
            return Err(invalid("App filter exceeds limit"));
        }
        for (name, value) in filter {
            let f = entity
                .fields
                .iter()
                .find(|f| f.name == *name && f.indexed && !f.translatable)
                .ok_or(invalid("Filter requires an indexed field"))?;
            let valid = match f.kind.as_str() {
                "string" => value.as_str().is_some_and(|v| v.len() <= 2000),
                "integer" => value.as_i64().is_some(),
                "boolean" => value.is_boolean(),
                _ => false,
            };
            if !valid {
                return Err(invalid("Invalid indexed app filter"));
            }
            filters.push(Filter {
                field: &f.name,
                value,
            });
        }
    }
    let publication = channel.and_then(|channel| {
        entity.publication.as_ref().map(|p| Window {
            status_field: &p.status_field,
            channel_field: &p.channel_field,
            starts_at: &p.starts_at,
            channel,
        })
    });
    Ok(Plan {
        limit,
        after,
        id,
        filters,
        publication,
    })
}
pub fn project(entity: &Entity, row: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("id".into(), row["id"].clone());
    out.insert("revision".into(), row["revision"].clone());
    for field in &entity.fields {
        out.insert(field.name.clone(), row[&field.name].clone());
    }
    Value::Object(out)
}
pub fn response(entity: &Entity, limit: i64, page: crate::queries::Page) -> Value {
    json!({"elements":page.rows.iter().map(|row|project(entity,row)).collect::<Vec<_>>(),"limit":limit,"hasMore":page.has_more,"nextCursor":page.next_cursor})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entity() -> Entity {
        serde_json::from_value(json!({"name":"notes","fields":[{"name":"title","kind":"string","indexed":true},{"name":"body","kind":"string","translatable":true}]})).unwrap()
    }
    #[test]
    fn original_page_filters_and_declared_projection_preserve_values() {
        let e = entity();
        let permissions = vec!["data.read".into()];
        let input = json!({"filter":{"title":"a"},"limit":2,"after":"one"});
        let plan = prepare(&permissions, &e, &input, None).unwrap();
        assert_eq!(plan.limit, 2);
        assert_eq!(plan.query().filters[0].field, "title");
        let row = json!({"id":"one","revision":3,"title":"","body":null,"secret":"hidden"});
        assert_eq!(
            project(&e, &row),
            json!({"id":"one","revision":3,"title":"","body":null})
        );
        for v in [
            json!({"limit":0}),
            json!({"limit":101}),
            json!({"filter":{"body":"a"}}),
            json!({"filter":{"title":4}}),
            json!({"id":""}),
        ] {
            assert!(prepare(&permissions, &e, &v, None).is_err());
        }
    }
}
