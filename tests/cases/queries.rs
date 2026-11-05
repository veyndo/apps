//! PostgreSQL keyset/equality and public window projections with hostile bound values and foreign namespaces.
use serde_json::json;
use sqlx::PgPool;
use veyndo_app_storage::{
    Error, RecordKey, Scope,
    queries::{self, Filter, Query, Window},
    records,
};
fn query() -> Query<'static> {
    Query {
        limit: 1,
        after: "",
        id: None,
        filters: &[],
        publication: None,
    }
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = Scope {
        tenant: "query-own",
        app: "pages",
    };
    let foreign = Scope {
        tenant: "query-foreign",
        app: "pages",
    };
    let another = Scope {
        tenant: "query-own",
        app: "other-app",
    };
    let hostile = "x' OR true--";
    for scope in [own, foreign, another] {
        let mut tx = pool.begin().await?;
        records::define(&mut tx, scope, "1", "pages", &json!({})).await?;
        for (id, status, channel, start, tag) in [
            ("a", "published", "web", None, "match"),
            ("b", "draft", "web", None, "match"),
            ("c", "published", "other", None, "match"),
            ("d", "published", "*", None, hostile),
            (
                "e",
                "published",
                "web",
                Some("2999-01-01T00:00:00Z"),
                "match",
            ),
        ] {
            let key = RecordKey {
                scope,
                entity: "pages",
                id,
            };
            records::put(&mut tx,key,1,&json!({"status":status,"channel":channel,"starts":start,"tag":tag,"active":true,"number":2})).await?;
            for (name, value) in [
                ("tag", json!(tag)),
                ("active", json!(true)),
                ("number", json!(2)),
            ] {
                sqlx::query("INSERT INTO app_record_indexes(tenant,app,entity,record_id,field,value_hash,value) VALUES($1,$2,'pages',$3,$4,sha256(jsonb_send($5::jsonb)),$5)")
                    .bind(scope.tenant).bind(scope.app).bind(id).bind(name).bind(value).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
    }
    let mut tx = pool.begin().await?;
    let page = queries::list(&mut tx, own, "pages", &query()).await?;
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0]["id"], "a");
    assert!(page.has_more);
    assert_eq!(page.next_cursor.as_deref(), Some("a"));
    let mut q = query();
    q.after = "a";
    q.limit = 100;
    let page = queries::list(&mut tx, own, "pages", &q).await?;
    assert_eq!(
        page.rows
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["b", "c", "d", "e"]
    );
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
    q.after = "";
    q.id = Some(hostile);
    assert!(
        queries::list(&mut tx, own, "pages", &q)
            .await?
            .rows
            .is_empty()
    );
    q.id = None;
    let value = json!(hostile);
    let filters = [Filter {
        field: "tag",
        value: &value,
    }];
    q.filters = &filters;
    let page = queries::list(&mut tx, own, "pages", &q).await?;
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0]["id"], "d");
    let key_value = json!("match");
    let truth = json!(true);
    let number = json!(2);
    let filters = [
        Filter {
            field: "tag",
            value: &key_value,
        },
        Filter {
            field: "active",
            value: &truth,
        },
        Filter {
            field: "number",
            value: &number,
        },
    ];
    q.filters = &filters;
    assert_eq!(
        queries::list(&mut tx, own, "pages", &q).await?.rows.len(),
        4
    );
    let wrong = json!("2");
    let filters = [Filter {
        field: "number",
        value: &wrong,
    }];
    q.filters = &filters;
    assert!(
        queries::list(&mut tx, own, "pages", &q)
            .await?
            .rows
            .is_empty()
    );
    q.filters = &[];
    q.publication = Some(Window {
        status_field: "status",
        channel_field: "channel",
        starts_at: "starts",
        channel: "web",
    });
    let page = queries::list(&mut tx, own, "pages", &q).await?;
    assert_eq!(
        page.rows
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["a", "d"]
    );
    q.publication = Some(Window {
        status_field: hostile,
        channel_field: "channel",
        starts_at: "starts",
        channel: hostile,
    });
    assert!(
        queries::list(&mut tx, own, "pages", &q)
            .await?
            .rows
            .is_empty()
    );
    q.publication = None;
    assert!(
        queries::list(
            &mut tx,
            Scope {
                tenant: "missing",
                app: "pages"
            },
            "pages",
            &q
        )
        .await?
        .rows
        .is_empty()
    );
    for scope in [foreign, another] {
        assert_eq!(
            queries::list(&mut tx, scope, "pages", &q).await?.rows.len(),
            5
        );
    }
    for limit in [0, 101, i64::MAX] {
        q.limit = limit;
        assert!(matches!(
            queries::list(&mut tx, own, "pages", &q).await,
            Err(Error::Invalid(_))
        ));
    }
    tx.rollback().await?;
    println!(
        "PASS record pages: bounded keysets, exact typed filters, hostile values/field names, public status/channel/schedule and scoped namespaces"
    );
    Ok(())
}
