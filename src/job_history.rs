//! Scoped bounded job history and detail projections; current package/actor rights and result redaction remain required host admission.
use crate::{Error, Scope};
use serde_json::Value;
use sqlx::{PgConnection, postgres::PgRow};
pub struct Query<'a> {
    pub limit: i64,
    pub after: Option<&'a str>,
    pub group: Option<&'a str>,
}
pub struct Page {
    /// Includes packageDigest only for the trusted host's current-package redaction.
    /// Never serialize directly to an untrusted consumer.
    pub rows: Vec<Value>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}
pub async fn list(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    query: &Query<'_>,
) -> Result<Page, Error> {
    if !(1..=100).contains(&query.limit)
        || [query.after, query.group]
            .into_iter()
            .flatten()
            .any(|s| s.is_empty() || s.len() > 100)
    {
        return Err(Error::Invalid(
            "Invalid app job page: limit 1..100; cursor/group 1..100 bytes",
        ));
    }
    // Resolve the scoped anchor once so concurrent archival cannot introduce NULL ordering.
    let before: Option<String> = if let Some(after) = query.after {
        Some(
            sqlx::query_scalar(
                "SELECT created_at::text FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3",
            )
            .bind(scope.tenant)
            .bind(scope.app)
            .bind(after)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Conflict(
                "App job cursor no longer exists; reload the job history",
            ))?,
        )
    } else {
        None
    };
    // Group is an untrusted correlation label, never authority or completion evidence.
    let mut rows: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',j.id,'action',j.action,'packageDigest',j.package_digest,'status',CASE WHEN j.status IN ('running','cancel_requested') AND j.lease_until<now() THEN 'uncertain' ELSE j.status END,'progress',j.progress,'message',j.message,'result',CASE WHEN j.status='succeeded' THEN j.result ELSE NULL END,'actor',j.actor,'revision',j.revision,'createdAt',j.created_at,'updatedAt',j.updated_at) FROM app_jobs j WHERE j.tenant=$1 AND j.app=$2 AND ($3::text IS NULL OR j.input->>'jobGroup'=$3) AND ($4::text IS NULL OR (j.created_at,j.id)<($5::text::timestamptz,$4)) ORDER BY j.created_at DESC,j.id DESC LIMIT $6")
        .bind(scope.tenant).bind(scope.app).bind(query.group).bind(query.after).bind(before).bind(query.limit + 1).fetch_all(tx).await?;
    let has_more = rows.len() > query.limit as usize;
    rows.truncate(query.limit as usize);
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
/// Trusted host must check the returned actor, current package and action rights before disclosing data.
pub async fn detail(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    id: &str,
) -> Result<Option<PgRow>, sqlx::Error> {
    sqlx::query("SELECT actor,action,package_digest,jsonb_build_object('id',id,'status',CASE WHEN status IN ('running','cancel_requested') AND lease_until<now() THEN 'uncertain' ELSE status END,'progress',progress,'message',message,'result',CASE WHEN status='succeeded' THEN result ELSE NULL END,'revision',revision) AS data FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3")
        .bind(scope.tenant).bind(scope.app).bind(id).fetch_optional(tx).await
}
