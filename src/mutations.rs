//! Shared record CAS/replay transaction; host admission, typed validation and audit hooks remain mandatory caller ports.
use crate::{RecordKey, records};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::future::Future;

#[derive(Debug)]
pub enum WriteError<E> {
    Database(sqlx::Error),
    Revision(i64),
    Conflict(&'static str),
    Invalid(&'static str),
    Hook(E),
}
impl<E> From<sqlx::Error> for WriteError<E> {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
/// Full fields have already passed the host's package, field, reference and language validation.
pub struct Mutation<'a> {
    pub key: RecordKey<'a>,
    pub expected_revision: i64,
    pub fields: &'a Value,
    pub declared_fields: &'a [&'a str],
    pub replay: bool,
}
/// Called inside the same transaction, after locking/CAS and around persistence.
/// The host owns workflow admission and bounded history. Failure must roll back the caller's transaction.
pub trait WriteHooks {
    type Error;
    fn before<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        old: Option<&'a Value>,
        fields: &'a Value,
        revision: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
    fn after<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        fields: &'a Value,
        revision: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}
/// Caller retains its package fence and transaction. No commits, SQL objects or identity inference here.
pub async fn write<H: WriteHooks>(
    tx: &mut PgConnection,
    mutation: &Mutation<'_>,
    hooks: &H,
) -> Result<Value, WriteError<H::Error>> {
    let key = mutation.key;
    if key.id.is_empty() || key.id.len() > 100 || !mutation.fields.is_object() {
        return Err(WriteError::Invalid("Record id and fields object required"));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,8))")
        .bind(records::lock_key(key))
        .execute(&mut *tx)
        .await?;
    let current = records::get(tx, key, true).await?;
    let revision = current
        .as_ref()
        .and_then(|r| r["revision"].as_i64())
        .unwrap_or(0);
    if mutation.replay
        && let Some(row) = &current
    {
        let projection = |v: &Value| -> Value {
            mutation
                .declared_fields
                .iter()
                .map(|name| ((*name).to_owned(), v[*name].clone()))
                .collect()
        };
        if projection(row) != projection(mutation.fields) {
            return Err(WriteError::Conflict(
                "Imported app content differs; review an explicit update",
            ));
        }
        return Ok(
            json!({"id":key.id,"revision":revision,"fields":mutation.fields,"replayed":true}),
        );
    }
    if revision != mutation.expected_revision {
        return Err(WriteError::Revision(revision));
    }
    let next = revision
        .checked_add(1)
        .ok_or(WriteError::Conflict("App record revision exhausted"))?;
    hooks
        .before(
            tx,
            current.as_ref(),
            mutation.fields,
            mutation.expected_revision,
        )
        .await
        .map_err(WriteError::Hook)?;
    records::put(tx, key, next, mutation.fields).await?;
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.record.changed',$2)")
        .bind(key.scope.tenant)
        .bind(json!({"app":key.scope.app,"entity":key.entity,"id":key.id,"revision":next}))
        .execute(&mut *tx)
        .await?;
    hooks
        .after(tx, mutation.fields, next)
        .await
        .map_err(WriteError::Hook)?;
    Ok(json!({"id":key.id,"revision":next,"fields":mutation.fields}))
}
