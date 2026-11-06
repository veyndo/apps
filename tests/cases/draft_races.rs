//! Two-connection autosave and per-actor quota races use the original PostgreSQL fences, not process-local locks.
use super::*;
use std::sync::Arc;
use tokio::sync::Barrier;
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    for revision in [0, 1] {
        let barrier = Arc::new(Barrier::new(2));
        let mut tasks = Vec::new();
        for text in ["left", "right"] {
            let pool = pool.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                save(&pool, "draft-race", "alice", "example", text, revision).await
            }));
        }
        let mut successes = 0;
        let mut conflicts = 0;
        for task in tasks {
            match task.await? {
                Ok(next) => {
                    assert_eq!(next, revision + 1);
                    successes += 1
                }
                Err(Error::Conflict(_)) => conflicts += 1,
                other => panic!("unexpected save result {other:?}"),
            }
        }
        assert_eq!((successes, conflicts), (1, 1));
    }
    for n in 0..19 {
        save(
            pool,
            "draft-quota",
            "alice",
            &format!("draft_{n}"),
            "seed",
            0,
        )
        .await?;
    }
    let barrier = Arc::new(Barrier::new(2));
    let mut tasks = Vec::new();
    for id in ["last_a", "last_b"] {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            save(&pool, "draft-quota", "alice", id, id, 0).await
        }));
    }
    let mut successes = 0;
    let mut denied = 0;
    for task in tasks {
        match task.await? {
            Ok(1) => successes += 1,
            Err(Error::Invalid("Maximum 20 private drafts; remove an old draft")) => denied += 1,
            other => panic!("unexpected quota result {other:?}"),
        }
    }
    assert_eq!((successes, denied), (1, 1));
    // Existing draft updates remain possible at quota; another actor has an independent budget.
    assert_eq!(
        save(
            pool,
            "draft-quota",
            "alice",
            "draft_0",
            "updated at quota",
            1
        )
        .await?,
        2
    );
    assert_eq!(
        save(pool, "draft-quota", "bob", "draft_0", "other user", 0).await?,
        1
    );
    let mut tx = pool.begin().await?;
    let owner = Owner {
        tenant: "draft-quota",
        actor: "alice",
    };
    assert_eq!(drafts::list(&mut tx, owner).await?.len(), 20);
    drafts::remove(&mut tx, owner, "draft_1").await?;
    tx.commit().await?;
    assert_eq!(
        save(pool, "draft-quota", "alice", "replacement", "new", 0).await?,
        1
    );
    // Revision exhaustion cannot wrap into a valid revision or partially overwrite a manifest.
    save(pool, "draft-overflow", "alice", "example", "original", 0).await?;
    sqlx::query("UPDATE developer_drafts SET revision=$1 WHERE tenant='draft-overflow'")
        .bind(i64::MAX)
        .execute(pool)
        .await?;
    assert!(matches!(
        save(
            pool,
            "draft-overflow",
            "alice",
            "example",
            "must not persist",
            i64::MAX
        )
        .await,
        Err(Error::Database(_))
    ));
    let mut tx = pool.begin().await?;
    let rows = drafts::list(
        &mut tx,
        Owner {
            tenant: "draft-overflow",
            actor: "alice",
        },
    )
    .await?;
    assert_eq!(rows[0]["revision"], i64::MAX);
    assert_eq!(rows[0]["manifest"], manifest("example", "original"));
    tx.rollback().await?;
    println!(
        "PASS private draft races: one CAS winner, actor quota contention, update at quota, freed capacity and revision exhaustion"
    );
    Ok(())
}
