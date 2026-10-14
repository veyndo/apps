//! Shared package fence prevents native record or document writes racing contract upgrades.
use super::*;
pub(crate) async fn lock_package_write(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
) -> Result<()> {
    // Match the install lock in shared mode: unrelated records remain concurrent, while upgrades cannot change the contract mid-write.
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended($1,7))")
        .bind(format!("{t}:{}", m.id))
        .execute(&mut **tx)
        .await?;
    let installed: Option<Value> = sqlx::query_scalar(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(t)
    .bind(&m.id)
    .fetch_optional(&mut **tx)
    .await?;
    let current: Manifest = serde_json::from_value(
        installed.ok_or(conflict("App is no longer active; reopen its view"))?,
    )
    .map_err(|_| bad("Invalid installed app contract"))?;
    if approval::canonical_digest(&current) != approval::canonical_digest(m) {
        return Err(conflict(
            "App package changed during this write; reopen its view",
        ));
    }
    Ok(())
}
