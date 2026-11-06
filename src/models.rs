//! Original fixed-table model installation; required host compatibility, required-field guard and projection rebuild retain their order.
use crate::{
    Scope,
    model::{Entity, manifest::Manifest},
    records,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
pub trait Compatibility: Sync {
    type Error: Send;
    fn check(&self, old: &Entity, next: &Entity) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum Error<E> {
    Database(sqlx::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    Host(E),
}
impl<E> From<sqlx::Error> for Error<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
/// Call only inside the already admitted installation transaction; no schema DDL, pool, identity or commit here.
pub async fn install<P: Compatibility>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    m: &Manifest,
    policy: &P,
) -> Result<(), Error<P::Error>> {
    if scope.app != m.id {
        return Err(Error::Invalid(
            "Model package identity must match admitted scope",
        ));
    }
    for e in &m.entities {
        let old = checked(tx, scope, e, policy).await?;
        records::define(tx, scope, &m.version, &e.name, &json!(e)).await?;
        if old.is_some_and(|old| old != json!(e)) {
            sqlx::query(
                "UPDATE app_records SET fields=fields WHERE tenant=$1 AND app=$2 AND entity=$3",
            )
            .bind(scope.tenant)
            .bind(scope.app)
            .bind(&e.name)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

async fn checked<P: Compatibility>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    e: &Entity,
    policy: &P,
) -> Result<Option<Value>, Error<P::Error>> {
    let old: Option<Value> = sqlx::query_scalar(
        "SELECT definition FROM app_models WHERE tenant=$1 AND app=$2 AND entity=$3",
    )
    .bind(scope.tenant)
    .bind(scope.app)
    .bind(&e.name)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(old) = &old {
        let previous: Entity = serde_json::from_value(old.clone())
            .map_err(|_| Error::Invalid("Invalid prior app model"))?;
        policy.check(&previous, e).map_err(Error::Host)?;
        if e.fields
            .iter()
            .any(|f| f.required && !previous.fields.iter().any(|p| p.name == f.name))
        {
            return Err(Error::Conflict(
                "New required field needs a data migration; add it nullable first",
            ));
        }
    }
    Ok(old)
}
/// Read-only compatibility review against actual installed model rows; no package approval or commit.
pub async fn review<P: Compatibility>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    m: &Manifest,
    policy: &P,
) -> Result<(), Error<P::Error>> {
    if scope.app != m.id {
        return Err(Error::Invalid(
            "Model package identity must match admitted scope",
        ));
    }
    for e in &m.entities {
        checked(tx, scope, e, policy).await?;
    }
    Ok(())
}
