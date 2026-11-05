//! Parameter-bound app record pages; the host admits model indexes, public scope and field projection.
use crate::{Error, Scope};
use serde_json::Value;
use sqlx::{PgConnection, Postgres, QueryBuilder, Row};
pub struct Filter<'a> {
    pub field: &'a str,
    pub value: &'a Value,
}
pub struct Window<'a> {
    pub status_field: &'a str,
    pub channel_field: &'a str,
    pub starts_at: &'a str,
    pub channel: &'a str,
}
pub struct Query<'a> {
    pub limit: i64,
    pub after: &'a str,
    pub id: Option<&'a str>,
    pub filters: &'a [Filter<'a>],
    pub publication: Option<Window<'a>>,
}
pub struct Page {
    pub rows: Vec<Value>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}
pub async fn list(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    entity: &str,
    input: &Query<'_>,
) -> Result<Page, Error> {
    if !(1..=100).contains(&input.limit) {
        return Err(Error::Invalid("App page limit must be 1..100"));
    }
    if input.after.len() > 100 {
        return Err(Error::Invalid("App cursor exceeds limit"));
    }
    if input.id.is_some_and(|s| s.is_empty() || s.len() > 100) {
        return Err(Error::Invalid("Invalid app record ID"));
    }
    if input.filters.len() > 4 {
        return Err(Error::Invalid("App filter exceeds limit"));
    }
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT fields||jsonb_build_object('id',id,'revision',revision) AS data FROM app_records r WHERE tenant=",
    );
    query
        .push_bind(scope.tenant)
        .push(" AND app=")
        .push_bind(scope.app)
        .push(" AND entity=")
        .push_bind(entity)
        .push(" AND id > ")
        .push_bind(input.after);
    if let Some(id) = input.id {
        query.push(" AND id = ").push_bind(id);
    }
    for filter in input.filters {
        if filter.field.is_empty()
            || filter.field.len() > 32
            || !(filter.value.as_str().is_some_and(|s| s.len() <= 2000)
                || filter.value.as_i64().is_some()
                || filter.value.is_boolean())
        {
            return Err(Error::Invalid("Invalid indexed app filter"));
        }
        query.push(" AND EXISTS(SELECT 1 FROM app_record_indexes ix WHERE ix.tenant=r.tenant AND ix.app=r.app AND ix.entity=r.entity AND ix.record_id=r.id AND ix.field=")
            .push_bind(filter.field).push(" AND ix.value_hash=sha256(jsonb_send(").push_bind(filter.value.clone())
            .push("::jsonb)) AND ix.value=").push_bind(filter.value.clone()).push("::jsonb)");
    }
    if let Some(window) = &input.publication {
        // Field names are values too. No manifest or channel bytes enter SQL syntax.
        query
            .push(" AND fields->>")
            .push_bind(window.status_field)
            .push("='published' AND (fields->>")
            .push_bind(window.channel_field)
            .push("='*' OR fields->>")
            .push_bind(window.channel_field)
            .push("=")
            .push_bind(window.channel)
            .push(") AND (fields->>")
            .push_bind(window.starts_at)
            .push(" IS NULL OR (fields->>")
            .push_bind(window.starts_at)
            .push(")::timestamptz<=now())");
    }
    query.push(" ORDER BY id LIMIT ").push_bind(input.limit + 1);
    let mut rows = query
        .build()
        .fetch_all(tx)
        .await?
        .iter()
        .map(|r| r.get::<Value, _>("data"))
        .collect::<Vec<_>>();
    let has_more = rows.len() > input.limit as usize;
    rows.truncate(input.limit as usize);
    let next_cursor = if has_more {
        rows.last()
            .and_then(|r| r["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    Ok(Page {
        rows,
        has_more,
        next_cursor,
    })
}
