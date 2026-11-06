//! Restoring history uses the actual shared CAS writer/history/outbox; rights, failed validation/hooks and competing restores cannot bypass it.
use super::*;
use sqlx::{PgConnection, Postgres, Transaction};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Barrier;
use veyndo_app_storage::{
    mutations::{self, Mutation, WriteHooks},
    record_history::{RestoreError, RestoreFuture, RestoreHost},
};
#[path = "record_restore_host.rs"]
mod host;
use host::{Audit, Host, counts};
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = key("history-restore", "example", "note");
    setup(pool, own.scope).await?;
    for (expected, text) in [(0, "first"), (1, "second")] {
        let mut tx = pool.begin().await?;
        mutations::write(
            &mut tx,
            &Mutation {
                key: own,
                expected_revision: expected,
                fields: &json!({"title":text}),
                declared_fields: &["title"],
                replay: false,
            },
            &Audit {
                key: own,
                fail_after: false,
            },
        )
        .await
        .unwrap();
        tx.commit().await?;
    }
    let calls = AtomicUsize::new(0);
    let host = Host {
        rights: Rights(&["catalog.read"]),
        expected: 2,
        calls: &calls,
        fail_after: false,
    };
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('veyndo.actor','restoring-editor',true),set_config('veyndo.source','app.history.restore',true)").execute(&mut *tx).await?;
    let result = history::restore(&mut tx, own, 1, &host).await.unwrap();
    assert_eq!(result["revision"], 3);
    assert_eq!(result["fields"]["title"], "first");
    tx.commit().await?;
    assert_eq!(counts(pool).await?, (3, 3, 3));
    let mut tx = pool.begin().await?;
    let rows = history::list(&mut tx, own, &host).await?;
    assert_eq!(
        rows[0].get::<Option<String>, _>("actor").as_deref(),
        Some("restoring-editor")
    );
    assert_eq!(rows[0].get::<String, _>("source"), "app.history.restore");
    assert_eq!(rows[1].get::<Value, _>("fields")["title"], "second");
    tx.rollback().await?;
    // Current historical read denial stops before the actual write hook.
    let denied = Host {
        rights: Rights(&[]),
        ..host
    };
    let before = calls.load(Ordering::SeqCst);
    let mut tx = pool.begin().await?;
    assert!(matches!(
        history::restore(&mut tx, own, 1, &denied).await,
        Err(RestoreError::Forbidden)
    ));
    for k in [
        key("other-history", "example", "note"),
        key(own.scope.tenant, "other", "note"),
        key(own.scope.tenant, own.scope.app, "other"),
    ] {
        assert!(matches!(
            history::restore(&mut tx, k, 1, &host).await,
            Err(RestoreError::Unavailable)
        ));
    }
    assert_eq!(calls.load(Ordering::SeqCst), before);
    tx.rollback().await?;
    // A retained historical version cannot overwrite a concurrent current edit.
    let mut tx = pool.begin().await?;
    assert!(
        matches!(history::restore(&mut tx,own,2,&host).await,Err(RestoreError::Host(s)) if s.contains("Revision(3)"))
    );
    tx.rollback().await?;
    assert_eq!(counts(pool).await?, (3, 3, 3));
    // Invalid retained scope/value shapes are never treated as current authority/validation.
    capture(
        pool,
        key(own.scope.tenant, own.scope.app, "malformed"),
        1,
        &json!({"title":"hidden"}),
        &json!([true]),
    )
    .await?;
    capture(
        pool,
        key(own.scope.tenant, own.scope.app, "invalid"),
        1,
        &json!({"title":42}),
        &json!(["catalog.read"]),
    )
    .await?;
    let current = Host {
        expected: 3,
        ..host
    };
    let mut tx = pool.begin().await?;
    assert!(matches!(
        history::restore(
            &mut tx,
            key(own.scope.tenant, own.scope.app, "malformed"),
            1,
            &current
        )
        .await,
        Err(RestoreError::Forbidden)
    ));
    assert!(
        matches!(history::restore(&mut tx,key(own.scope.tenant,own.scope.app,"invalid"),1,&current).await,Err(RestoreError::Host(s)) if s.contains("typed field rule"))
    );
    tx.rollback().await?;
    let baseline = counts(pool).await?;
    let failing = Host {
        fail_after: true,
        ..current
    };
    let mut tx = pool.begin().await?;
    assert!(
        matches!(history::restore(&mut tx,own,2,&failing).await,Err(RestoreError::Host(s)) if s.contains("audit rejected"))
    );
    tx.rollback().await?;
    assert_eq!(counts(pool).await?, baseline);
    // Two actual connections restore against revision 3. The shared writer serializes CAS.
    let barrier = Arc::new(Barrier::new(2));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            let calls = AtomicUsize::new(0);
            let host = Host {
                rights: Rights(&["catalog.read"]),
                expected: 3,
                calls: &calls,
                fail_after: false,
            };
            let mut tx = pool.begin().await.unwrap();
            barrier.wait().await;
            let result =
                history::restore(&mut tx, key("history-restore", "example", "note"), 2, &host)
                    .await;
            if result.is_ok() {
                tx.commit().await.unwrap();
            } else {
                tx.rollback().await.unwrap();
            }
            result
        }));
    }
    let mut success = 0;
    let mut conflict = 0;
    for task in tasks {
        match task.await? {
            Ok(value) => {
                assert_eq!(value["revision"], 4);
                success += 1;
            }
            Err(RestoreError::Host(s)) if s.contains("Revision(4)") => conflict += 1,
            other => panic!("unexpected restore result {other:?}"),
        }
    }
    assert_eq!((success, conflict), (1, 1));
    // Actual capture failure rolls back an already attempted current row and outbox write.
    sqlx::query("ALTER TABLE app_record_versions ADD CONSTRAINT fixture_history_failure CHECK(record_id<>'reject-history')").execute(pool).await?;
    let failed = key(own.scope.tenant, own.scope.app, "reject-history");
    let before =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox WHERE tenant='history-restore'")
            .fetch_one(pool)
            .await?;
    let mut tx = pool.begin().await?;
    assert!(
        mutations::write(
            &mut tx,
            &Mutation {
                key: failed,
                expected_revision: 0,
                fields: &json!({"title":"denied"}),
                declared_fields: &["title"],
                replay: false
            },
            &Audit {
                key: failed,
                fail_after: false
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    assert!(records::get(&mut tx, failed, false).await?.is_none());
    tx.rollback().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox WHERE tenant='history-restore'")
            .fetch_one(pool)
            .await?,
        before
    );
    println!(
        "PASS history restoration: actual CAS writer/history/outbox, new revision, current historical rights/fields, failed hooks/capture rollback and competing restores"
    );
    Ok(())
}
