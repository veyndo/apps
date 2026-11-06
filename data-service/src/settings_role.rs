//! Exact optional settings privileges; package authority, encryption keys and other secret kinds remain unavailable to the serving login.
pub async fn check(pool: &sqlx::PgPool) -> Result<(), &'static str> {
    let allowed:bool=sqlx::query_scalar(r#"SELECT
      has_table_privilege('app_service_secrets','SELECT') AND has_table_privilege('app_service_secrets','INSERT')
      AND has_column_privilege('app_service_secrets','ciphertext','UPDATE')
      AND has_column_privilege('app_service_secrets','package_digest','UPDATE')
      AND has_column_privilege('app_service_secrets','revision','UPDATE')
      AND has_column_privilege('app_service_secrets','actor','UPDATE')
      AND has_column_privilege('app_service_secrets','rotated_at','UPDATE')
      AND NOT has_table_privilege('app_service_secrets','DELETE,TRUNCATE,REFERENCES,TRIGGER')
      AND NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='app_service_secrets'::regclass AND attnum>0 AND NOT attisdropped AND attname IN ('tenant','app','kind') AND has_column_privilege(attrelid,attnum,'UPDATE'))
      AND EXISTS(SELECT 1 FROM pg_class WHERE oid='app_service_secrets'::regclass AND relrowsecurity AND relforcerowsecurity)
    "#).fetch_one(pool).await.map_err(|_|"Unsupported App settings profile")?;
    if !allowed {
        return Err("App settings writer has missing or excessive privileges");
    }
    Ok(())
}
