//! Required synthetic restore host validates current fields and uses the actual shared CAS writer, audit history and outbox.
use super::*;
pub(super) struct Audit<'a> {
    pub(super) key: RecordKey<'a>,
    pub(super) fail_after: bool,
}
impl WriteHooks for Audit<'_> {
    type Error = String;
    async fn before(
        &self,
        tx: &mut PgConnection,
        old: Option<&Value>,
        _: &Value,
        revision: i64,
    ) -> Result<(), String> {
        if let Some(old) = old {
            history::capture(
                tx,
                &Capture {
                    key: self.key,
                    revision,
                    fields: &json!({"title":old["title"]}),
                    read_scopes: &json!(["catalog.read"]),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    async fn after(
        &self,
        tx: &mut PgConnection,
        fields: &Value,
        revision: i64,
    ) -> Result<(), String> {
        history::capture(
            tx,
            &Capture {
                key: self.key,
                revision,
                fields,
                read_scopes: &json!(["catalog.read"]),
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        if self.fail_after {
            return Err("fixture audit rejected".into());
        }
        Ok(())
    }
}
pub(super) struct Host<'a> {
    pub(super) rights: Rights,
    pub(super) expected: i64,
    pub(super) calls: &'a AtomicUsize,
    pub(super) fail_after: bool,
}
impl ReadAuthority for Host<'_> {
    fn allowed(&self, scope: &str) -> bool {
        self.rights.allowed(scope)
    }
}
impl<'db> RestoreHost<'db> for Host<'_> {
    type Error = String;
    fn save<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        key: RecordKey<'a>,
        fields: &'a Value,
    ) -> RestoreFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if !fields["title"].is_string() {
                return Err("current typed field rule rejected historical value".into());
            }
            mutations::write(
                tx,
                &Mutation {
                    key,
                    expected_revision: self.expected,
                    fields,
                    declared_fields: &["title"],
                    replay: false,
                },
                &Audit {
                    key,
                    fail_after: self.fail_after,
                },
            )
            .await
            .map_err(|e| format!("{e:?}"))
        })
    }
}
pub(super) async fn counts(pool: &PgPool) -> Result<(i64, i64, i64), sqlx::Error> {
    sqlx::query_as("SELECT (SELECT revision FROM app_records WHERE tenant='history-restore' AND app='example' AND entity='notes' AND id='note'),(SELECT count(*) FROM app_record_versions WHERE tenant='history-restore'),(SELECT count(*) FROM outbox WHERE tenant='history-restore')").fetch_one(pool).await
}
