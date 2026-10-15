//! Protect newly installed app tables within their schema transaction; preserve pre-099 historical migration replay.
use super::*;
pub(super) async fn attach(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
) -> Result<()> {
    let available: bool = sqlx::query_scalar(
        "SELECT to_regprocedure('public.veyndo_guard_app_table(text)') IS NOT NULL",
    )
    .fetch_one(&mut **tx)
    .await?;
    if available {
        sqlx::query("SELECT public.veyndo_guard_app_table($1)")
            .bind(table)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
