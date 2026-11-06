//! Actual historical snapshots, original read scopes, immutable attribution, scoped retention and reconnect without a Core process.
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use veyndo_app_storage::{
    Error, RecordKey, Scope,
    record_history::{self as history, Capture, ReadAuthority},
    records,
};
#[path = "record_restore.rs"]
mod restoration;
#[path = "record_history_retention.rs"]
mod retention;
pub(super) struct Rights(pub &'static [&'static str]);
impl ReadAuthority for Rights {
    fn allowed(&self, scope: &str) -> bool {
        self.0.contains(&scope)
    }
}
pub(super) fn key<'a>(tenant: &'a str, app: &'a str, id: &'a str) -> RecordKey<'a> {
    RecordKey {
        scope: Scope { tenant, app },
        entity: "notes",
        id,
    }
}
pub(super) async fn setup(pool: &PgPool, scope: Scope<'_>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO app_packages(tenant,id,manifest) VALUES($1,$2,'{}') ON CONFLICT DO NOTHING",
    )
    .bind(scope.tenant)
    .bind(scope.app)
    .execute(&mut *tx)
    .await?;
    records::define(
        &mut tx,
        scope,
        "1.0.0",
        "notes",
        &json!({"fields":[{"name":"title","kind":"string"}]}),
    )
    .await?;
    tx.commit().await
}
pub(super) async fn capture(
    pool: &PgPool,
    key: RecordKey<'_>,
    revision: i64,
    fields: &Value,
    scopes: &Value,
) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    history::capture(
        &mut tx,
        &Capture {
            key,
            revision,
            fields,
            read_scopes: scopes,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = key("history-own", "example", "same");
    let foreign = key("history-foreign", "example", "same");
    let app = key("history-own", "another", "same");
    for k in [own, foreign, app] {
        setup(pool, k.scope).await?;
    }
    let fields = json!({"title":{"es-ES":"Primero","de-DE":"Erste","it-IT":"","pl-PL":null},"old_private":"not a public projection"});
    let scopes = json!(["catalog.read", "customers.pii"]);
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('veyndo.actor','history-editor',true),set_config('veyndo.source','fixture.save',true)").execute(&mut *tx).await?;
    history::capture(
        &mut tx,
        &Capture {
            key: own,
            revision: 1,
            fields: &fields,
            read_scopes: &scopes,
        },
    )
    .await?;
    tx.commit().await?;
    // A repeated capture cannot silently weaken original rights or replace its fields/attribution.
    capture(
        pool,
        own,
        1,
        &json!({"title":"must not replace"}),
        &json!([]),
    )
    .await?;
    for k in [foreign, app] {
        capture(pool, k, 1, &json!({"title":k.scope.tenant}), &json!([])).await?;
    }
    let all = Rights(&["catalog.read", "customers.pii"]);
    let limited = Rights(&["catalog.read"]);
    let mut tx = pool.begin().await?;
    assert!(history::list(&mut tx, own, &limited).await?.is_empty());
    let rows = history::list(&mut tx, own, &all).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<Value, _>("fields"), fields);
    assert_eq!(rows[0].get::<Value, _>("read_scopes"), scopes);
    assert_eq!(
        rows[0].get::<Option<String>, _>("actor").as_deref(),
        Some("history-editor")
    );
    assert_eq!(rows[0].get::<String, _>("source"), "fixture.save");
    assert!(rows[0].get::<String, _>("created_at").contains('-'));
    assert_eq!(
        history::list(&mut tx, foreign, &limited).await?[0].get::<Value, _>("fields")["title"],
        "history-foreign"
    );
    assert_eq!(history::list(&mut tx, app, &limited).await?.len(), 1);
    assert!(
        history::list(
            &mut tx,
            key(own.scope.tenant, own.scope.app, "missing"),
            &all
        )
        .await?
        .is_empty()
    );
    tx.rollback().await?;
    for (revision, malformed) in [(2, json!({})), (3, json!(["catalog.read", false]))] {
        capture(pool, own, revision, &json!({"title":"hidden"}), &malformed).await?;
    }
    let mut tx = pool.begin().await?;
    assert_eq!(history::list(&mut tx, own, &all).await?.len(), 1);
    history::capture(
        &mut tx,
        &Capture {
            key: own,
            revision: 4,
            fields: &fields,
            read_scopes: &scopes,
        },
    )
    .await?;
    assert_eq!(history::list(&mut tx, own, &all).await?.len(), 2);
    tx.rollback().await?;
    assert!(matches!(
        capture(pool, own, 0, &fields, &scopes).await,
        Err(Error::Invalid(_))
    ));
    let reopened = PgPool::connect_with(pool.connect_options().as_ref().clone()).await?;
    let mut tx = reopened.begin().await?;
    assert_eq!(history::list(&mut tx, own, &all).await?.len(), 1);
    tx.rollback().await?;
    reopened.close().await;
    // Scope derivation includes only current list actions for this entity, in original order.
    let m: veyndo_app_storage::model::manifest::Manifest = serde_json::from_value(json!({
    "id":"example","version":"1.0.0","coreApi":"v1","runtime":"native","name":{"en":"Example"},"permissions":[],
    "actions":[
        {"name":"list","description":"List","handler":"list","entity":"notes","inputSchema":{},"permission":"catalog.read","requiredPermissions":["customers.pii"]},
        {"name":"save","description":"Save","handler":"save","entity":"notes","inputSchema":{},"permission":"catalog.write"},
        {"name":"other","description":"Other","handler":"list","entity":"different","inputSchema":{},"permission":"orders.read"}
    ]}))?;
    assert_eq!(history::read_scopes(&m, "notes"), scopes);
    retention::run(pool).await?;
    restoration::run(pool).await?;
    println!(
        "PASS record history: retained original rights/attribution, immutable captures, scoped/malformed reads, rollback/reconnect and required restore host"
    );
    Ok(())
}
