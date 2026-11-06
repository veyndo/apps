//! Independent job history: bounded stable cursors, scoped namespaces, hostile labels and uncertainty/result projection.
use serde_json::json;
use sqlx::{PgPool, Row};
use veyndo_app_storage::{
    Error, Scope,
    job_history::{self, Query},
};
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = Scope {
        tenant: "history",
        app: "example",
    };
    for (tenant, app) in [
        ("history", "example"),
        ("history", "other"),
        ("elsewhere", "example"),
    ] {
        for id in ["a", "b", "c"] {
            sqlx::query("INSERT INTO app_jobs(tenant,app,id,action,actor,permission,package_digest,request_key,input_digest,input,created_at,status,result) VALUES($1,$2,$3,'sync','owner','apps.manage','reviewed',$3,'input',$4,'2026-11-06T00:00:00Z','succeeded',$5)")
                .bind(tenant).bind(app).bind(id).bind(json!({"jobGroup":"x' OR true--"})).bind(json!({"private":tenant})).execute(pool).await?;
        }
    }
    let mut tx = pool.begin().await?;
    let mut q = Query {
        limit: 2,
        after: None,
        group: None,
    };
    let first = job_history::list(&mut tx, own, &q).await?;
    assert!(first.has_more);
    assert_eq!(
        first
            .rows
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["c", "b"]
    );
    assert_eq!(first.next_cursor.as_deref(), Some("b"));
    assert_eq!(first.rows[0]["result"], json!({"private":"history"}));
    assert_eq!(first.rows[0]["packageDigest"], "reviewed");
    q.after = Some("b");
    let second = job_history::list(&mut tx, own, &q).await?;
    assert!(!second.has_more);
    assert!(second.next_cursor.is_none());
    assert_eq!(second.rows.len(), 1);
    assert_eq!(second.rows[0]["id"], "a");
    q.after = None;
    q.group = Some("x' OR true--");
    assert_eq!(job_history::list(&mut tx, own, &q).await?.rows.len(), 2);
    q.group = Some("missing");
    assert!(job_history::list(&mut tx, own, &q).await?.rows.is_empty());
    q.group = None;
    for limit in [0, 101, i64::MAX] {
        q.limit = limit;
        assert!(matches!(
            job_history::list(&mut tx, own, &q).await,
            Err(Error::Invalid(_))
        ));
    }
    q.limit = 2;
    for label in [String::new(), "x".repeat(101), "ä".repeat(51)] {
        for (after, group) in [(None, Some(label.as_str())), (Some(label.as_str()), None)] {
            let invalid = Query {
                limit: 2,
                after,
                group,
            };
            assert!(matches!(
                job_history::list(&mut tx, own, &invalid).await,
                Err(Error::Invalid(_))
            ));
        }
    }
    q.after = Some("not-present");
    assert!(matches!(
        job_history::list(&mut tx, own, &q).await,
        Err(Error::Conflict(_))
    ));
    assert!(
        job_history::detail(&mut tx, own, "missing")
            .await?
            .is_none()
    );
    tx.commit().await?;
    sqlx::query("INSERT INTO app_jobs(tenant,app,id,action,actor,permission,package_digest,request_key,input_digest,input) VALUES('elsewhere','example','foreign-only','sync','owner','apps.manage','reviewed','foreign-only','input','{}')").execute(pool).await?;
    let mut tx = pool.begin().await?;
    q.after = Some("foreign-only");
    assert!(matches!(
        job_history::list(&mut tx, own, &q).await,
        Err(Error::Conflict(_))
    ));
    assert!(
        job_history::detail(&mut tx, own, "foreign-only")
            .await?
            .is_none()
    );
    sqlx::query("UPDATE app_jobs SET status='running',lease_until=now()-interval '1 second' WHERE tenant='history' AND app='example' AND id='b'").execute(&mut *tx).await?;
    let row = job_history::detail(&mut tx, own, "b").await?.unwrap();
    let data: serde_json::Value = row.get("data");
    assert_eq!(data["status"], "uncertain");
    assert!(data["result"].is_null());
    q.after = None;
    assert!(
        job_history::list(&mut tx, own, &q)
            .await?
            .rows
            .iter()
            .any(|r| r["id"] == "b" && r["status"] == "uncertain" && r["result"].is_null())
    );
    tx.commit().await?;
    println!(
        "PASS independent job history: bounded/tied/scoped cursors, foreign anchors, hostile groups and uncertainty/result projection"
    );
    Ok(())
}
