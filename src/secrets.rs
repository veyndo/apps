//! Digest-bound encrypted app configuration and transactional rotation; host identity, applicability and actual authenticated encryption are required.
use crate::Scope;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
/// Host key management and authenticated encryption have no fallback implementation.
pub trait Crypto: Sync {
    type Error: Send;
    fn seal(&self, binding: &str, plaintext: &str) -> Result<String, Self::Error>;
    fn open(&self, binding: &str, ciphertext: &str) -> Result<String, Self::Error>;
}
#[derive(Debug)]
pub enum SecretError<E> {
    Database(sqlx::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    Crypto(E),
}
impl<E> From<sqlx::Error> for SecretError<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
/// Trusted host admitted current package, personal actor, kind and payload; not a request DTO.
pub struct Write<'a> {
    pub kind: &'a str,
    pub digest: &'a str,
    pub revision: i64,
    pub actor: &'a str,
    pub secret: &'a str,
}
pub struct Metadata {
    pub revision: i64,
    pub digest: String,
}
fn binding(scope: Scope<'_>, kind: &str, digest: &str, revision: i64) -> String {
    format!(
        "app:{}:{}:{kind}:{digest}:{revision}",
        scope.tenant, scope.app
    )
}
pub async fn metadata(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    kind: &str,
) -> Result<Option<Metadata>, sqlx::Error> {
    Ok(sqlx::query("SELECT revision,package_digest FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3")
        .bind(scope.tenant).bind(scope.app).bind(kind).fetch_optional(tx).await?
        .map(|row| Metadata { revision:row.get("revision"),digest:row.get("package_digest") }))
}
/// Metadata only; ciphertext and plaintext have no public projection here.
pub async fn list(tx: &mut PgConnection, scope: Scope<'_>) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'revision',revision,'rotatedAt',rotated_at,'packageDigest',package_digest) FROM app_service_secrets WHERE tenant=$1 AND app=$2 ORDER BY kind")
        .bind(scope.tenant).bind(scope.app).fetch_all(tx).await
}
/// Caller owns transaction and rollback on every error; seal, CAS/package checks, write and event retain their original order.
pub async fn rotate<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    write: &Write<'_>,
    crypto: &C,
) -> Result<i64, SecretError<C::Error>> {
    let next = write
        .revision
        .checked_add(1)
        .filter(|n| *n > 0)
        .ok_or(SecretError::Invalid("Secret revision exhausted or invalid"))?;
    let encrypted = crypto
        .seal(
            &binding(scope, write.kind, write.digest, next),
            write.secret,
        )
        .map_err(SecretError::Crypto)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,844))")
        .bind(format!("{}:{}:{}", scope.tenant, scope.app, write.kind))
        .execute(&mut *tx)
        .await?;
    let current:Option<i64>=sqlx::query_scalar("SELECT revision FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3 FOR UPDATE")
        .bind(scope.tenant).bind(scope.app).bind(write.kind).fetch_optional(&mut *tx).await?;
    if current.unwrap_or(0) != write.revision {
        return Err(SecretError::Conflict(
            "Secret was rotated elsewhere; reload its revision",
        ));
    }
    let active_digest: String = sqlx::query_scalar(
        "SELECT digest FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(scope.tenant)
    .bind(scope.app)
    .fetch_one(&mut *tx)
    .await?;
    if write.digest != active_digest {
        return Err(SecretError::Conflict(
            "App package changed before secret rotation",
        ));
    }
    sqlx::query("INSERT INTO app_service_secrets(tenant,app,kind,ciphertext,package_digest,revision,actor) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(tenant,app,kind) DO UPDATE SET ciphertext=EXCLUDED.ciphertext,package_digest=EXCLUDED.package_digest,revision=EXCLUDED.revision,actor=EXCLUDED.actor,rotated_at=now()")
        .bind(scope.tenant).bind(scope.app).bind(write.kind).bind(encrypted).bind(write.digest).bind(next).bind(write.actor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.secret_rotated',$2)")
        .bind(scope.tenant)
        .bind(json!({"app":scope.app,"kind":write.kind,"revision":next,"actor":write.actor}))
        .execute(tx)
        .await?;
    Ok(next)
}
async fn read<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    kind: &str,
    digest: &str,
    crypto: &C,
    locked: bool,
) -> Result<Option<String>, SecretError<C::Error>> {
    let sql = if locked {
        "SELECT ciphertext,package_digest,revision FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3 FOR SHARE"
    } else {
        "SELECT ciphertext,package_digest,revision FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3"
    };
    let Some(row) = sqlx::query(sql)
        .bind(scope.tenant)
        .bind(scope.app)
        .bind(kind)
        .fetch_optional(tx)
        .await?
    else {
        return Ok(None);
    };
    if row.get::<String, _>("package_digest") != digest {
        return Err(SecretError::Conflict(
            "App secret belongs to an older package; review and rotate it for this version",
        ));
    }
    crypto
        .open(
            &binding(scope, kind, digest, row.get("revision")),
            &row.get::<String, _>("ciphertext"),
        )
        .map(Some)
        .map_err(SecretError::Crypto)
}
/// Internal service resolution after host admission; never return this plaintext over a public API.
pub async fn resolve<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    kind: &str,
    digest: &str,
    crypto: &C,
) -> Result<Option<String>, SecretError<C::Error>> {
    read(tx, scope, kind, digest, crypto, false).await
}
/// Use within original webhook receipt/intake transaction so rotation cannot race verification.
pub async fn resolve_locked<C: Crypto>(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    kind: &str,
    digest: &str,
    crypto: &C,
) -> Result<Option<String>, SecretError<C::Error>> {
    read(tx, scope, kind, digest, crypto, true).await
}
