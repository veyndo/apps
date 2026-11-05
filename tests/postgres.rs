//! Actual independent PostgreSQL persistence: composite scope, atomic handoff, replay, quota, restart and lease fences.
use serde_json::json;
use sqlx::{PgPool, Row};
use veyndo_app_storage::{RecordKey, Scope, jobs, progress, records};
fn job<'a>(
    scope: Scope<'a>,
    id: &'a str,
    key: &'a str,
    input: &'a serde_json::Value,
) -> jobs::NewJob<'a> {
    jobs::NewJob {
        scope,
        id,
        request_key: key,
        input,
        action: "sync",
        actor: "owner",
        permission: "apps.manage",
        package_digest: "reviewed-package",
        event_kind: "app.example.job_sync",
    }
}
#[tokio::test]
#[ignore = "requires an explicitly selected disposable APP_STORAGE_TEST_DATABASE_URL"]
async fn independent_postgres_contract() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("APP_STORAGE_TEST_DATABASE_URL")?;
    let pool = PgPool::connect(&url).await?;
    sqlx::raw_sql(include_str!("schema.sql"))
        .execute(&pool)
        .await?;
    let own = Scope {
        tenant: "own",
        app: "example",
    };
    let foreign = Scope {
        tenant: "foreign",
        app: "example",
    };
    let other_app = Scope {
        tenant: "own",
        app: "another",
    };
    let input = json!({"kind":"orders"});
    let definition = json!({"fields":[{"name":"title","kind":"string","translatable":true}]});
    for scope in [own, foreign, other_app] {
        let mut tx = pool.begin().await?;
        records::define(&mut tx, scope, "1.0.0", "offers", &definition).await?;
        let key = RecordKey {
            scope,
            entity: "offers",
            id: "same-id",
        };
        records::put(
            &mut tx,
            key,
            1,
            &json!({"title":{"fr-FR":scope.tenant,"it-IT":null}}),
        )
        .await?;
        tx.commit().await?;
    }
    let mut tx = pool.begin().await?;
    let key = RecordKey {
        scope: own,
        entity: "offers",
        id: "same-id",
    };
    assert_eq!(
        records::get(&mut tx, key, true).await?.unwrap()["title"]["fr-FR"],
        "own"
    );
    assert_eq!(
        records::rows(&mut tx, foreign, "offers", 1).await?[0]["title"]["fr-FR"],
        "foreign"
    );
    records::put(&mut tx, key, 2, &json!({"title":{"fr-FR":"rolled back"}})).await?;
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    assert_eq!(
        records::get(&mut tx, key, false).await?.unwrap()["revision"],
        1
    );
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    let first = jobs::enqueue(&mut tx, &job(own, "one", "key-one", &input)).await?;
    assert!(!first["reused"].as_bool().unwrap());
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    let replay = jobs::enqueue(&mut tx, &job(own, "ignored-new-id", "key-one", &input)).await?;
    assert_eq!(first["jobId"], replay["jobId"]);
    assert_eq!(replay["reused"], true);
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        jobs::enqueue(
            &mut tx,
            &job(own, "changed", "key-one", &json!({"kind":"listing"}))
        )
        .await,
        Err(veyndo_app_storage::Error::Conflict(_))
    ));
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    jobs::enqueue(&mut tx, &job(own, "rolled-back", "rollback", &input)).await?;
    tx.rollback().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox")
            .fetch_one(&pool)
            .await?,
        1
    );
    // Two independent connections race the same request; PostgreSQL owns serialization.
    let mut handles = Vec::new();
    for id in ["race-a", "race-b"] {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let mut tx = pool.begin().await.unwrap();
            let value = json!({"kind":"orders"});
            let result = jobs::enqueue(
                &mut tx,
                &job(
                    Scope {
                        tenant: "race",
                        app: "example",
                    },
                    id,
                    "race-key",
                    &value,
                ),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            result
        }));
    }
    let left = handles.remove(0).await?;
    let right = handles.remove(0).await?;
    assert_eq!(left["jobId"], right["jobId"]);
    assert_ne!(left["reused"], right["reused"]);
    let mut tx = pool.begin().await?;
    assert!(jobs::load(&mut tx, foreign, "one").await?.is_none());
    assert!(!jobs::claim(&mut tx, foreign, "one", "foreign-lease").await?);
    assert!(jobs::claim(&mut tx, own, "one", "lease-one").await?);
    assert!(!jobs::claim(&mut tx, own, "one", "another-lease").await?);
    tx.commit().await?;
    let mut expired = pool.begin().await?;
    jobs::enqueue(&mut expired, &job(own, "expired", "expired-key", &input)).await?;
    jobs::claim(&mut expired, own, "expired", "expired-lease").await?;
    expired.commit().await?;
    sqlx::query("UPDATE app_jobs SET lease_until=now()-interval '1 second' WHERE tenant='own' AND id='expired'").execute(&pool).await?;
    let empty = json!({});
    let mut expired = pool.begin().await?;
    assert!(
        !progress::advance(
            &mut expired,
            own,
            "expired",
            &progress::Progress {
                lease: "expired-lease",
                status: "succeeded",
                progress: 100,
                message: &empty,
                result: &empty,
                checkpoint: None
            }
        )
        .await?
    );
    assert!(!jobs::claim(&mut expired, own, "expired", "reclaim").await?);
    expired.rollback().await?;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(&pool)
        .await?;
    let mut rejected = job(own, "outbox-failure", "outbox-key", &input);
    rejected.event_kind = "reject-fixture";
    let mut rejected_tx = pool.begin().await?;
    assert!(matches!(
        jobs::enqueue(&mut rejected_tx, &rejected).await,
        Err(veyndo_app_storage::Error::Database(_))
    ));
    rejected_tx.rollback().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app_jobs WHERE id='outbox-failure'")
            .fetch_one(&pool)
            .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox")
            .fetch_one(&pool)
            .await?,
        before
    );
    pool.close().await;
    let pool = PgPool::connect(&url).await?; // Restart the host connection; state is not process-local.
    let mut tx = pool.begin().await?;
    let row = jobs::load(&mut tx, own, "one").await?.unwrap();
    assert_eq!(row.get::<String, _>("lease"), "lease-one");
    let empty = json!({});
    let mut update = progress::Progress {
        lease: "wrong",
        status: "succeeded",
        progress: 100,
        message: &empty,
        result: &empty,
        checkpoint: None,
    };
    assert!(!progress::advance(&mut tx, own, "one", &update).await?);
    update.lease = "lease-one";
    update.status = "running";
    update.progress = 50;
    assert!(progress::advance(&mut tx, own, "one", &update).await?);
    update.progress = 49;
    assert!(!progress::advance(&mut tx, own, "one", &update).await?);
    tx.commit().await?;
    sqlx::query("UPDATE app_jobs SET status='cancel_requested' WHERE tenant='own' AND id='one'")
        .execute(&pool)
        .await?;
    let mut tx = pool.begin().await?;
    update.status = "succeeded";
    update.progress = 100;
    assert!(!progress::advance(&mut tx, own, "one", &update).await?);
    update.status = "cancelled";
    assert!(progress::advance(&mut tx, own, "one", &update).await?);
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    assert!(!jobs::claim(&mut tx, own, "one", "reclaimed").await?);
    tx.commit().await?;
    for index in 0..10 {
        let mut tx = pool.begin().await?;
        jobs::enqueue(
            &mut tx,
            &job(
                Scope {
                    tenant: "quota",
                    app: "example",
                },
                &format!("job-{index}"),
                &format!("key-{index}"),
                &input,
            ),
        )
        .await?;
        tx.commit().await?;
    }
    let mut tx = pool.begin().await?;
    assert!(matches!(
        jobs::enqueue(
            &mut tx,
            &job(
                Scope {
                    tenant: "quota",
                    app: "example"
                },
                "overflow",
                "key-11",
                &input
            )
        )
        .await,
        Err(veyndo_app_storage::Error::Quota(_))
    ));
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    records::reset_preview(&mut tx, own).await?;
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    assert!(records::get(&mut tx, key, false).await?.is_none());
    assert!(
        records::get(
            &mut tx,
            RecordKey {
                scope: other_app,
                ..key
            },
            false
        )
        .await?
        .is_some()
    );
    assert!(
        records::get(
            &mut tx,
            RecordKey {
                scope: foreign,
                ..key
            },
            false
        )
        .await?
        .is_some()
    );
    tx.commit().await?;
    println!(
        "PASS independent PostgreSQL: record scope/rollback, atomic job/outbox, concurrent replay, conflict, lease, cancel, quota, restart and scoped preview reset"
    );
    Ok(())
}
