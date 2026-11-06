//! Exact managed-write privilege profile; no arbitrary table, package-definition, schema or historical-update rights.
use sqlx::PgPool;
pub async fn check(pool: &PgPool, installs: bool) -> Result<(), &'static str> {
    let allowed:bool=sqlx::query_scalar(r#"SELECT
  has_table_privilege('app_records','SELECT') AND has_table_privilege('app_records','INSERT') AND has_column_privilege('app_records','fields','UPDATE') AND has_column_privilege('app_records','revision','UPDATE')
  AND NOT has_table_privilege('app_records','DELETE,TRUNCATE,REFERENCES,TRIGGER')
  AND NOT EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid='app_records'::regclass AND a.attnum>0 AND NOT a.attisdropped AND a.attname NOT IN ('fields','revision') AND has_column_privilege(a.attrelid,a.attnum,'UPDATE'))
  AND has_column_privilege('app_packages','active','UPDATE') AND has_sequence_privilege('outbox_id_seq','USAGE')
  AND NOT has_table_privilege('app_packages','DELETE,TRUNCATE,REFERENCES,TRIGGER') AND ($1 OR NOT has_table_privilege('app_packages','INSERT'))
  AND NOT EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid='app_packages'::regclass AND a.attnum>0 AND NOT a.attisdropped AND a.attname<>'active' AND NOT ($1 AND a.attname IN ('version','manifest','digest','revision')) AND has_column_privilege(a.attrelid,a.attnum,'UPDATE'))
  AND EXISTS(SELECT 1 FROM pg_policy WHERE polrelid='app_packages'::regclass AND polname='writer_package_lock' AND pg_get_expr(polwithcheck,polrelid)='false')
  AND has_table_privilege('app_record_versions','SELECT') AND has_table_privilege('app_record_versions','INSERT') AND has_table_privilege('app_record_versions','DELETE') AND NOT has_table_privilege('app_record_versions','UPDATE,TRUNCATE,REFERENCES,TRIGGER') AND NOT has_any_column_privilege('app_record_versions','UPDATE,REFERENCES')
  AND has_table_privilege('outbox','INSERT') AND NOT has_table_privilege('outbox','SELECT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER') AND NOT has_any_column_privilege('outbox','SELECT,UPDATE,REFERENCES')
  AND (SELECT count(*)=2 FROM pg_class WHERE oid IN ('app_record_versions'::regclass,'outbox'::regclass) AND relrowsecurity AND relforcerowsecurity)
 "#).bind(installs).fetch_one(pool).await.map_err(|_|"Unsupported managed write profile")?;
    if !allowed {
        return Err("Managed writer has missing or excessive privileges");
    }
    Ok(())
}
