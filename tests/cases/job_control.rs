//! Independent control transaction: exact review/CAS fences, mandatory retry authority, rollback and competing merchants.
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use veyndo_app_storage::{
    Scope,
    job_control::{self, Context, ControlError, RetryAdmission, RetryFuture},
};
struct Host {
    denied: bool,
    event: &'static str,
}
impl<'db> RetryAdmission<'db> for Host {
    type Error = &'static str;
    fn admit<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        row: &'a PgRow,
    ) -> RetryFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move {
            assert_eq!(row.get::<String, _>("action"), "sync");
            assert_eq!(row.get::<Value, _>("input")["jobGroup"], "synthetic");
            sqlx::query(
                "INSERT INTO outbox(tenant,kind,data) VALUES('control','host.admission','{}')",
            )
            .execute(&mut **tx)
            .await
            .map_err(|_| "host db")?;
            if self.denied {
                return Err("current rights denied");
            }
            Ok(self.event.into())
        })
    }
}
fn ctx(scope: Scope<'_>) -> Context<'_> {
    Context {
        scope,
        actor: Some("current-owner"),
        package_digest: "new-reviewed-package",
    }
}
async fn seed(
    pool: &PgPool,
    tenant: &str,
    app: &str,
    id: &str,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app_jobs(tenant,app,id,action,actor,permission,package_digest,request_key,input_digest,input,status,lease,lease_until,started_at,progress) VALUES($1,$2,$3,'sync','old-owner','apps.manage','old-package',$3,'input',$4,$5,'old-lease',now()+interval '60 seconds',now(),42)")
        .bind(tenant).bind(app).bind(id).bind(json!({"jobGroup":"synthetic"})).bind(status).execute(pool).await?;
    Ok(())
}
async fn state(pool: &PgPool, id: &str) -> Result<(String, i64), sqlx::Error> {
    sqlx::query_as(
        "SELECT status,revision FROM app_jobs WHERE tenant='control' AND app='example' AND id=$1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = Scope {
        tenant: "control",
        app: "example",
    };
    let host = Host {
        denied: false,
        event: "app.example.job_sync",
    };
    seed(pool, "control", "example", "cancel", "queued").await?;
    seed(pool, "control", "other", "cancel", "queued").await?;
    seed(pool, "foreign-control", "example", "cancel", "queued").await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        job_control::control(&mut tx, &ctx(own), "missing", &json!({}), &host).await,
        Err(ControlError::NotFound)
    ));
    assert!(matches!(
        job_control::control(&mut tx, &ctx(own), "cancel", &json!({"revision":99}), &host).await,
        Err(ControlError::Conflict("App job revision changed"))
    ));
    assert!(matches!(
        job_control::control(&mut tx, &ctx(own), "cancel", &json!({"revision":1}), &host).await,
        Err(ControlError::Invalid("Job operation required"))
    ));
    assert!(matches!(
        job_control::control(
            &mut tx,
            &ctx(own),
            "cancel",
            &json!({"revision":1,"operation":"archive"}),
            &host
        )
        .await,
        Err(ControlError::Conflict(_))
    ));
    job_control::control(
        &mut tx,
        &ctx(own),
        "cancel",
        &json!({"revision":1,"operation":"cancel"}),
        &host,
    )
    .await
    .unwrap();
    tx.commit().await?;
    assert_eq!(state(pool, "cancel").await?, ("cancelled".into(), 2));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_jobs WHERE id='cancel' AND status='queued'"
        )
        .fetch_one(pool)
        .await?,
        2
    );
    let mut tx = pool.begin().await?;
    job_control::control(
        &mut tx,
        &ctx(own),
        "cancel",
        &json!({"revision":2,"operation":"archive"}),
        &host,
    )
    .await
    .unwrap();
    tx.rollback().await?;
    assert_eq!(state(pool, "cancel").await?.1, 2);
    let mut tx = pool.begin().await?;
    job_control::control(
        &mut tx,
        &ctx(own),
        "cancel",
        &json!({"revision":2,"operation":"archive"}),
        &host,
    )
    .await
    .unwrap();
    tx.commit().await?;
    assert!(state(pool, "cancel").await.is_err());
    for (id, status) in [
        ("running", "running"),
        ("failed", "failed"),
        ("unknown", "uncertain"),
        ("expired", "running"),
    ] {
        seed(pool, "control", "example", id, status).await?;
    }
    sqlx::query("UPDATE app_jobs SET lease_until=now()-interval '1 second' WHERE tenant='control' AND id='expired'").execute(pool).await?;
    let mut tx = pool.begin().await?;
    job_control::control(
        &mut tx,
        &ctx(own),
        "running",
        &json!({"revision":1,"operation":"cancel"}),
        &host,
    )
    .await
    .unwrap();
    let row =
        sqlx::query("SELECT status,lease FROM app_jobs WHERE tenant='control' AND id='running'")
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(row.get::<String, _>("status"), "cancel_requested");
    assert_eq!(row.get::<String, _>("lease"), "old-lease");
    tx.commit().await?;
    for id in ["failed", "unknown", "expired"] {
        let mut tx = pool.begin().await?;
        assert!(matches!(
            job_control::control(
                &mut tx,
                &ctx(own),
                id,
                &json!({"revision":1,"operation":"retry"}),
                &host
            )
            .await,
            Err(ControlError::Conflict(
                "Confirm external outcome before retrying or resolving this job"
            ))
        ));
        tx.rollback().await?;
        let mut tx = pool.begin().await?;
        job_control::control(
            &mut tx,
            &ctx(own),
            id,
            &json!({"revision":1,"operation":"retry","approveUnknownOutcome":true}),
            &host,
        )
        .await
        .unwrap();
        tx.commit().await?;
        let row=sqlx::query("SELECT actor,package_digest,lease,lease_until IS NULL AND started_at IS NULL AS reset,progress,status FROM app_jobs WHERE tenant='control' AND app='example' AND id=$1").bind(id).fetch_one(pool).await?;
        assert_eq!(row.get::<String, _>("actor"), "current-owner");
        assert_eq!(
            row.get::<String, _>("package_digest"),
            "new-reviewed-package"
        );
        assert_eq!(row.get::<String, _>("status"), "queued");
        assert_eq!(row.get::<i32, _>("progress"), 0);
        assert!(row.get::<bool, _>("reset"));
        assert!(row.get::<Option<String>, _>("lease").is_none());
    }
    seed(pool, "control", "example", "denied", "failed").await?;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(pool)
        .await?;
    for reject in ["hook", "outbox", "actor"] {
        let mut tx = pool.begin().await?;
        let mut context = ctx(own);
        if reject == "actor" {
            context.actor = None;
        }
        let h = Host {
            denied: reject == "hook",
            event: if reject == "outbox" {
                "reject-fixture"
            } else {
                "app.example.job_sync"
            },
        };
        let result = job_control::control(
            &mut tx,
            &context,
            "denied",
            &json!({"revision":1,"operation":"retry","approveUnknownOutcome":true}),
            &h,
        )
        .await;
        match reject {
            "hook" => assert!(matches!(
                result,
                Err(ControlError::Hook("current rights denied"))
            )),
            "outbox" => assert!(matches!(result, Err(ControlError::Database(_)))),
            _ => assert!(matches!(
                result,
                Err(ControlError::Invalid("Personal account required"))
            )),
        }
        tx.rollback().await?;
        assert_eq!(state(pool, "denied").await?, ("failed".into(), 1));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox")
                .fetch_one(pool)
                .await?,
            before
        );
    }
    seed(pool, "control", "example", "resolve", "uncertain").await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        job_control::control(
            &mut tx,
            &ctx(own),
            "resolve",
            &json!({"revision":1,"operation":"resolve_cancelled"}),
            &host
        )
        .await,
        Err(ControlError::Conflict(_))
    ));
    job_control::control(
        &mut tx,
        &ctx(own),
        "resolve",
        &json!({"revision":1,"operation":"resolve_cancelled","approveUnknownOutcome":true}),
        &host,
    )
    .await
    .unwrap();
    tx.commit().await?;
    assert_eq!(state(pool, "resolve").await?, ("cancelled".into(), 2));
    seed(pool, "control", "example", "race", "queued").await?;
    let mut tasks = vec![];
    for _ in 0..2 {
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move {
            let mut tx = pool.begin().await.unwrap();
            let h = Host {
                denied: false,
                event: "app.example.job_sync",
            };
            let result = job_control::control(
                &mut tx,
                &ctx(Scope {
                    tenant: "control",
                    app: "example",
                }),
                "race",
                &json!({"revision":1,"operation":"cancel"}),
                &h,
            )
            .await;
            let accepted = result.is_ok();
            if accepted {
                tx.commit().await.unwrap();
            } else {
                assert!(matches!(
                    result,
                    Err(ControlError::Conflict("App job revision changed"))
                ));
                tx.rollback().await.unwrap();
            }
            accepted
        }));
    }
    assert_ne!(tasks.remove(0).await?, tasks.remove(0).await?);
    assert_eq!(state(pool, "race").await?, ("cancelled".into(), 2));
    println!(
        "PASS independent job control: CAS, review/retry/resolve, actor/package reset, retained cancel lease, hook/outbox/actor rollback, scope and race"
    );
    Ok(())
}
