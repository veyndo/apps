//! Exact optional installer DML profile; immutable history and SQL/schema/owner authority stay separated.
pub async fn check(pool: &sqlx::PgPool) -> Result<(), &'static str> {
    let allowed:bool=sqlx::query_scalar(r#"SELECT
 has_table_privilege('app_packages','INSERT') AND has_column_privilege('app_packages','version','UPDATE') AND has_column_privilege('app_packages','manifest','UPDATE') AND has_column_privilege('app_packages','digest','UPDATE') AND has_column_privilege('app_packages','revision','UPDATE')
 AND NOT has_table_privilege('app_packages','DELETE,TRUNCATE,REFERENCES,TRIGGER')
 AND NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='app_packages'::regclass AND attnum>0 AND NOT attisdropped AND attname IN ('tenant','id') AND has_column_privilege(attrelid,attnum,'UPDATE'))
 AND has_table_privilege('app_models','INSERT') AND has_column_privilege('app_models','version','UPDATE') AND has_column_privilege('app_models','definition','UPDATE')
 AND NOT has_table_privilege('app_models','DELETE,TRUNCATE,REFERENCES,TRIGGER')
 AND NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='app_models'::regclass AND attnum>0 AND NOT attisdropped AND attname IN ('tenant','app','entity') AND has_column_privilege(attrelid,attnum,'UPDATE'))
 AND has_table_privilege('app_versions','SELECT') AND has_table_privilege('app_versions','INSERT') AND NOT has_table_privilege('app_versions','UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER') AND NOT has_any_column_privilege('app_versions','UPDATE,REFERENCES')
 AND has_table_privilege('app_storage_usage','SELECT') AND has_column_privilege('app_storage_usage','tenant','INSERT') AND has_column_privilege('app_storage_usage','app','INSERT')
 AND NOT has_table_privilege('app_storage_usage','UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER') AND NOT has_any_column_privilege('app_storage_usage','UPDATE,REFERENCES')
 AND NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='app_storage_usage'::regclass AND attnum>0 AND NOT attisdropped AND attname NOT IN ('tenant','app') AND has_column_privilege(attrelid,attnum,'INSERT'))
 AND (SELECT count(*)=2 FROM pg_class WHERE oid IN ('app_versions'::regclass,'app_storage_usage'::regclass) AND relrowsecurity AND relforcerowsecurity)
 AND (SELECT count(*)=2 FROM pg_trigger WHERE tgrelid='app_records'::regclass AND tgname IN ('app_data_budget','app_data_version') AND tgenabled='O')
 "#).fetch_one(pool).await.map_err(|_|"Unsupported App installation profile")?;
    if !allowed {
        return Err("App installer has missing or excessive privileges");
    }
    Ok(())
}
