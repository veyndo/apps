//! Exact opt-in job-supervisor privileges; no enqueue, progress/result writes, key edits or schema authority.
pub async fn check(pool: &sqlx::PgPool) -> Result<(), &'static str> {
    let valid:bool=sqlx::query_scalar(r#"SELECT
 has_table_privilege('app_jobs','SELECT') AND has_table_privilege('app_jobs','DELETE')
 AND NOT has_table_privilege('app_jobs','INSERT,TRUNCATE,REFERENCES,TRIGGER') AND NOT has_any_column_privilege('app_jobs','INSERT,REFERENCES')
 AND NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='app_jobs'::regclass AND attnum>0 AND NOT attisdropped AND has_column_privilege(attrelid,attnum,'UPDATE')<>(attname IN ('status','lease','lease_until','actor','package_digest','progress','started_at','revision','updated_at')))
 AND (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE oid='app_jobs'::regclass)
 "#).fetch_one(pool).await.map_err(|_|"Unsupported job supervisor profile")?;
    if !valid {
        return Err("Job supervisor has missing or excessive privileges");
    }
    Ok(())
}
