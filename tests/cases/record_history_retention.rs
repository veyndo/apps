//! Real per-record 20-revision and per-app 16 MiB history retention preserve other apps/tenants and roll back pruning.
use super::*;
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = key("history-retention", "example", "one");
    setup(pool, own.scope).await?;
    for revision in 1..=25 {
        capture(pool, own, revision, &json!({"title":revision}), &json!([])).await?;
    }
    let mut tx = pool.begin().await?;
    let rows = history::list(&mut tx, own, &Rights(&[])).await?;
    assert_eq!(rows.len(), 20);
    assert_eq!(rows[0].get::<i64, _>("revision"), 25);
    assert_eq!(rows[19].get::<i64, _>("revision"), 6);
    // Capture plus pruning is one caller transaction: rolling back restores the dropped oldest version.
    history::capture(
        &mut tx,
        &Capture {
            key: own,
            revision: 26,
            fields: &json!({"title":26}),
            read_scopes: &json!([]),
        },
    )
    .await?;
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    assert_eq!(
        history::list(&mut tx, own, &Rights(&[])).await?[19].get::<i64, _>("revision"),
        6
    );
    tx.rollback().await?;
    // Deterministic incompressible fields exercise the original pg_column_size budget, not a toy row count.
    use sha2::{Digest, Sha256};
    let text = (0u32..8192)
        .map(|i| format!("{:x}", Sha256::digest(i.to_le_bytes())))
        .collect::<String>();
    let fields = json!({"title":text});
    let bounded = key("history-byte-budget", "example", "record");
    setup(pool, bounded.scope).await?;
    let other = key("history-byte-budget", "another", "untouched");
    setup(pool, other.scope).await?;
    capture(pool, other, 1, &json!({"title":"other app"}), &json!([])).await?;
    let foreign = key("history-byte-foreign", "example", "untouched");
    setup(pool, foreign.scope).await?;
    capture(
        pool,
        foreign,
        1,
        &json!({"title":"foreign tenant"}),
        &json!([]),
    )
    .await?;
    for i in 0..75 {
        capture(
            pool,
            key(
                bounded.scope.tenant,
                bounded.scope.app,
                &format!("record_{i:03}"),
            ),
            1,
            &fields,
            &json!([]),
        )
        .await?;
    }
    let bytes:i64=sqlx::query_scalar("SELECT coalesce(sum(pg_column_size(fields)),0)::bigint FROM app_record_versions WHERE tenant=$1 AND app=$2")
        .bind(bounded.scope.tenant).bind(bounded.scope.app).fetch_one(pool).await?;
    assert!(bytes > 0 && bytes <= 16777216, "retained bytes {bytes}");
    let mut tx = pool.begin().await?;
    assert!(
        history::list(
            &mut tx,
            key(bounded.scope.tenant, bounded.scope.app, "record_000"),
            &Rights(&[])
        )
        .await?
        .is_empty()
    );
    assert_eq!(
        history::list(
            &mut tx,
            key(bounded.scope.tenant, bounded.scope.app, "record_074"),
            &Rights(&[])
        )
        .await?
        .len(),
        1
    );
    assert_eq!(history::list(&mut tx, other, &Rights(&[])).await?.len(), 1);
    assert_eq!(
        history::list(&mut tx, foreign, &Rights(&[])).await?.len(),
        1
    );
    tx.rollback().await?;
    println!(
        "PASS history retention: newest 20 revisions, actual 16 MiB byte cap, scoped pruning and rollback"
    );
    Ok(())
}
