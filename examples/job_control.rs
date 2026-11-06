//! Executable synthetic job review on the disposable test schema: failure, explicit retry, cancel, archive and caller rollback.
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use veyndo_app_storage::{
    Scope,
    job_control::{self, Context, RetryAdmission, RetryFuture},
    job_history, jobs, progress,
};
struct FixtureAdmission;
impl<'db> RetryAdmission<'db> for FixtureAdmission {
    type Error = &'static str;
    fn admit<'a>(
        &'a self,
        _tx: &'a mut Transaction<'db, Postgres>,
        row: &'a PgRow,
    ) -> RetryFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move {
            if row.get::<String, _>("action") != "sync"
                || row.get::<serde_json::Value, _>("input") != json!({"fixture":true})
            {
                return Err("unsupported synthetic job");
            }
            Ok("app.example.job_sync".into())
        })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let mut tx = pool.begin().await?;
    let tenant = format!(
        "job-example-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let scope = Scope {
        tenant: &tenant,
        app: "example",
    };
    let empty = json!({});
    jobs::enqueue(
        &mut tx,
        &jobs::NewJob {
            scope,
            id: "example-job",
            action: "sync",
            actor: "fixture-owner",
            permission: "apps.manage",
            package_digest: "fixture-package",
            request_key: "example-key",
            input: &json!({"fixture":true}),
            event_kind: "app.example.job_sync",
        },
    )
    .await?;
    assert!(jobs::claim(&mut tx, scope, "example-job", "fixture-lease").await?);
    assert!(
        progress::advance(
            &mut tx,
            scope,
            "example-job",
            &progress::Progress {
                lease: "fixture-lease",
                status: "failed",
                progress: 20,
                message: &empty,
                result: &empty,
                checkpoint: None
            }
        )
        .await?
    );
    let context = Context {
        scope,
        actor: Some("fixture-owner"),
        package_digest: "fixture-package",
    };
    let row = job_history::detail(&mut tx, scope, "example-job")
        .await?
        .unwrap();
    let revision = row.get::<serde_json::Value, _>("data")["revision"]
        .as_i64()
        .unwrap();
    job_control::control(
        &mut tx,
        &context,
        "example-job",
        &json!({
            "revision":revision,"operation":"retry","approveUnknownOutcome":true
        }),
        &FixtureAdmission,
    )
    .await
    .map_err(|e| format!("fixture retry: {e:?}"))?;
    job_control::control(
        &mut tx,
        &context,
        "example-job",
        &json!({"revision":revision+1,"operation":"cancel"}),
        &FixtureAdmission,
    )
    .await
    .map_err(|e| format!("fixture cancel: {e:?}"))?;
    job_control::control(
        &mut tx,
        &context,
        "example-job",
        &json!({"revision":revision+2,"operation":"archive"}),
        &FixtureAdmission,
    )
    .await
    .map_err(|e| format!("fixture archive: {e:?}"))?;
    assert!(
        job_history::detail(&mut tx, scope, "example-job")
            .await?
            .is_none()
    );
    tx.rollback().await?;
    println!(
        "PASS independent job review: failure, acknowledged retry, cancel, archive; fixture state rolled back"
    );
    Ok(())
}
