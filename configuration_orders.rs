//! Paginated app-specific immutable purchase projections; filtering happens before LIMIT and never exposes buyer PII.
use super::*;
pub(crate) async fn configurations(a: &App, t: &str, id: &str, input: &Value) -> Result<Value> {
    let limit = input["limit"].as_i64().unwrap_or(100);
    let after = input["after"].as_str().unwrap_or("");
    if !(1..=100).contains(&limit) || after.len() > 100 {
        return Err(bad("Invalid configuration order page"));
    }
    let legacy = json!({"cart":{"lineItems":[{"configuration":{"app":id}}]}});
    let multiple = json!({"cart":{"lineItems":[{"appConfigurations":[{"app":id}]}]}});
    let rows = sqlx::query("SELECT id,created_at,data->>'orderNumber' AS number,data->'cart'->'lineItems' AS items FROM orders WHERE tenant=$1 AND (data @> $2::jsonb OR data @> $3::jsonb) AND ($4='' OR (created_at,id)<(SELECT created_at,id FROM orders WHERE tenant=$1 AND id=$4)) ORDER BY created_at DESC,id DESC LIMIT $5")
        .bind(t).bind(legacy).bind(multiple).bind(after).bind(limit+1).fetch_all(&a.db).await?;
    let has_more = rows.len() > limit as usize;
    let mut orders = vec![];
    for row in rows.iter().take(limit as usize) {
        let items: Value = row.get("items");
        let items = items.as_array().ok_or(bad("Invalid order configuration snapshot"))?.iter().filter(|item| item["configuration"]["app"] == id || item["appConfigurations"].as_array().is_some_and(|v| v.iter().any(|c| c["app"] == id))).map(|item| {
            // Return only this app's configuration even when multiple apps share a position.
            let configurations = item["appConfigurations"].as_array().map(|v| v.iter().filter(|c| c["app"] == id).cloned().collect::<Vec<_>>()).unwrap_or_else(|| vec![item["configuration"].clone()]);
            json!({"referencedId":item["referencedId"],"label":item["label"],"quantity":item["quantity"],"configuration":configurations.first(),"appConfigurations":configurations})
        }).collect::<Vec<_>>();
        orders.push(json!({"orderId":row.get::<String,_>("id"),"orderNumber":row.get::<Option<String>,_>("number"),"items":items}));
    }
    let next = if has_more {
        orders.last().and_then(|v| v["orderId"].as_str())
    } else {
        None
    };
    Ok(json!({"elements":orders,"hasMore":has_more,"nextCursor":next,"limit":limit}))
}
