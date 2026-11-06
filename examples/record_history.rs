//! Runnable bounded history and required restore authority on a disposable schema; no implicit writer, identities or commit.
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use veyndo_app_storage::{
    RecordKey, Scope,
    record_history::{
        self as history, Capture, ReadAuthority, RestoreError, RestoreFuture, RestoreHost,
    },
};
struct Host {
    may_read: bool,
}
impl ReadAuthority for Host {
    fn allowed(&self, scope: &str) -> bool {
        self.may_read && scope == "catalog.read"
    }
}
impl<'db> RestoreHost<'db> for Host {
    type Error = &'static str;
    fn save<'a>(
        &'a self,
        _: &'a mut Transaction<'db, Postgres>,
        _: RecordKey<'a>,
        _: &'a Value,
    ) -> RestoreFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async {
            Err(
                "Supply the actual admitted current-package/field/reference/CAS writer before restoring",
            )
        })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let tenant = format!(
        "history-example-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let key = RecordKey {
        scope: Scope {
            tenant: &tenant,
            app: "example",
        },
        entity: "notes",
        id: "one",
    };
    let mut tx = pool.begin().await?;
    // Synthetic existing package in the test schema. A real host owns package installation and current identity.
    sqlx::query("INSERT INTO app_packages(tenant,id,manifest) VALUES($1,'example','{}')")
        .bind(&tenant)
        .execute(&mut *tx)
        .await?;
    for revision in 1..=2 {
        history::capture(&mut tx,&Capture {key,revision,fields:&json!({"title":{"es-ES":format!("Versión {revision}"),"de-DE":"Entwurf","it-IT":""}}),read_scopes:&json!(["catalog.read"])}).await?;
    }
    assert_eq!(
        history::list(&mut tx, key, &Host { may_read: true })
            .await?
            .len(),
        2
    );
    assert!(
        history::list(&mut tx, key, &Host { may_read: false })
            .await?
            .is_empty()
    );
    assert!(matches!(
        history::restore(&mut tx, key, 1, &Host { may_read: false }).await,
        Err(RestoreError::Forbidden)
    ));
    assert!(matches!(
        history::restore(&mut tx, key, 1, &Host { may_read: true }).await,
        Err(RestoreError::Host(_))
    ));
    tx.rollback().await?;
    println!(
        "PASS record history: bounded scoped captures, current historical rights and no restoration without an actual admitted writer; rolled back"
    );
    Ok(())
}
