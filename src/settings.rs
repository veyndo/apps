//! Declared settings use one encrypted CAS transaction and secret-safe response; current administrator/live/package admission stays with the caller.
use crate::model::{self, manifest::Manifest};
use crate::{
    Scope,
    secrets::{self, Crypto, SecretError},
};
use serde_json::{Value, json};
use sqlx::PgConnection;
/// Already admitted update context; values are supplied separately and never confused with a transport credential.
pub struct Update<'a> {
    pub digest: &'a str,
    pub revision: i64,
    pub actor: &'a str,
}
#[derive(Debug)]
pub enum Error<E> {
    Storage(SecretError<E>),
    Model(model::Error),
}
impl<E> From<SecretError<E>> for Error<E> {
    fn from(e: SecretError<E>) -> Self {
        Self::Storage(e)
    }
}
async fn values<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    m: &Manifest,
    digest: &str,
    crypto: &C,
) -> Result<Value, Error<C::Error>> {
    let Some(encoded) = secrets::resolve(tx, scope, "settings", digest, crypto).await? else {
        return Ok(json!({}));
    };
    let encoded = zeroize::Zeroizing::new(encoded);
    let values: Value = serde_json::from_str(&encoded)
        .map_err(|_| Error::Storage(SecretError::Invalid("Invalid encrypted app settings")))?;
    Ok(model::settings::declared(&m.settings, &values))
}
pub async fn read<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    m: &Manifest,
    crypto: &C,
) -> Result<Value, Error<C::Error>> {
    let digest = model::manifest::canonical_digest(m);
    let row = secrets::metadata(tx, scope, "settings")
        .await
        .map_err(SecretError::Database)?;
    let revision = row.as_ref().map(|r| r.revision).unwrap_or(0);
    let current = row.as_ref().is_none_or(|r| r.digest == digest);
    let values = if current {
        values(tx, scope, m, &digest, crypto).await?
    } else {
        json!({})
    };
    Ok(
        json!({"fields":m.settings,"values":model::settings::visible(&m.settings,&values),"configured":model::settings::configured(&m.settings,&values),"revision":revision,"digest":digest,"current":current}),
    )
}
/// The host validates explicit approval/current digest and supplies the admitted actor. No commit occurs here.
pub async fn save<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    m: &Manifest,
    write: &Update<'_>,
    input: &Value,
    crypto: &C,
) -> Result<i64, Error<C::Error>> {
    let digest = model::manifest::canonical_digest(m);
    if write.digest != digest {
        return Err(Error::Storage(SecretError::Conflict(
            "Approve the current package before saving settings",
        )));
    }
    let current = if secrets::metadata(tx, scope, "settings")
        .await
        .map_err(SecretError::Database)?
        .is_some_and(|row| row.digest == digest)
    {
        values(tx, scope, m, &digest, crypto).await?
    } else {
        json!({})
    };
    let encoded = zeroize::Zeroizing::new(
        model::settings::prepare(&m.settings, &current, input).map_err(Error::Model)?,
    );
    Ok(secrets::rotate(
        tx,
        scope,
        &secrets::Write {
            kind: "settings",
            digest: &digest,
            revision: write.revision,
            actor: write.actor,
            secret: &encoded,
        },
        crypto,
    )
    .await?)
}
