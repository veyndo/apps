//! Private provenance-bearing app exports feed merchant retrieval and durable app events; never public PDP answers.
use super::*;
pub(crate) async fn collect_sources(a: &App) -> Result<()> {
    let configured: Value = serde_json::from_str(&env::var("APP_SERVICES").unwrap_or("{}".into()))
        .map_err(|_| bad("Invalid app services"))?;
    let ids = configured
        .as_object()
        .map(|v| v.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let rows=sqlx::query("SELECT p.tenant,p.id,coalesce(c.cursor,0) AS cursor FROM app_packages p LEFT JOIN app_export_cursors c ON c.tenant=p.tenant AND c.app=p.id WHERE p.active AND p.id=ANY($1) AND p.manifest->'permissions' ? 'knowledge.write' ORDER BY coalesce(c.last_poll,'epoch'),p.tenant,p.id LIMIT 100").bind(ids).fetch_all(&a.db).await?;
    let mut tasks = tokio::task::JoinSet::new();
    for row in rows {
        let app = a.clone();
        tasks.spawn(async move { collect_one(&app, row).await });
        if tasks.len() >= 8 {
            let _ = tasks.join_next().await;
        }
    }
    while tasks.join_next().await.is_some() {}

    Ok(())
}
async fn collect_one(a: &App, r: sqlx::postgres::PgRow) -> Result<()> {
    let t: String = r.get("tenant");
    let id: String = r.get("id");
    let cursor: i64 = r.get("cursor");
    if staging::parent(a, &t).await?.is_some() {
        return Ok(());
    }
    sqlx::query("INSERT INTO app_export_cursors(tenant,app,last_poll) VALUES($1,$2,now()) ON CONFLICT(tenant,app) DO UPDATE SET last_poll=now()").bind(&t).bind(&id).execute(&a.db).await?;
    let result = gateway::service_call(a, &t, &id, "exports", &json!({"cursor":cursor})).await;
    if let Ok(v) = result
        && ingest(a, &t, &id, cursor, &v).await.is_err()
    {
        eprintln!("App export rejected: {id}");
    }
    Ok(())
}

async fn ingest(a: &App, t: &str, app: &str, previous: i64, v: &Value) -> Result<()> {
    let cursor = v["cursor"]
        .as_i64()
        .filter(|n| *n >= previous)
        .ok_or(bad("Invalid export cursor"))?;
    let sources = v["sources"]
        .as_array()
        .filter(|s| s.len() <= 10)
        .ok_or(bad("Invalid app sources"))?;
    if sources.is_empty() {
        return Ok(());
    }
    let mut tx = a.db.begin().await?;
    // AGE MERGE has no unique constraint: serialize shared Shop-node writes across apps.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,39))")
        .bind(t)
        .execute(&mut *tx)
        .await?;
    let current: i64 = sqlx::query_scalar(
        "SELECT coalesce((SELECT cursor FROM app_export_cursors WHERE tenant=$1 AND app=$2),0)",
    )
    .bind(t)
    .bind(app)
    .fetch_one(&mut *tx)
    .await?;
    if current != previous {
        return Ok(());
    }
    for s in sources {
        let id = s["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 150)
            .ok_or(bad("Invalid source ID"))?;
        if s["deleted"] == true {
            sqlx::query("DELETE FROM app_evidence WHERE tenant=$1 AND app=$2 AND source_id=$3")
                .bind(t)
                .bind(app)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            knowledge::sync_integration(&mut tx, t, app, id, s, true).await?;
            continue;
        }
        let title = s["title"]
            .as_str()
            .filter(|s| s.len() <= 300)
            .ok_or(bad("Invalid source title"))?;
        let text = s["text"]
            .as_str()
            .filter(|s| s.len() <= 12000)
            .ok_or(bad("Invalid source text"))?;
        let url = s["sourceUrl"]
            .as_str()
            .filter(|s| s.starts_with("https://") && s.len() <= 1500)
            .ok_or(bad("Invalid source URL"))?;
        let kind = s["kind"]
            .as_str()
            .filter(|s| s.len() <= 50)
            .ok_or(bad("Invalid source kind"))?;
        let digest = hash(&s.to_string());
        let n=sqlx::query("INSERT INTO app_evidence(tenant,app,source_id,kind,title,body,source_url,metadata,digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(tenant,app,source_id) DO UPDATE SET title=EXCLUDED.title,body=EXCLUDED.body,source_url=EXCLUDED.source_url,metadata=EXCLUDED.metadata,digest=EXCLUDED.digest,updated_at=now() WHERE app_evidence.digest<>EXCLUDED.digest").bind(t).bind(app).bind(id).bind(kind).bind(title).bind(text).bind(url).bind(&s["metadata"]).bind(&digest).execute(&mut *tx).await?.rows_affected();
        if n > 0 {
            knowledge::sync_integration(&mut tx, t, app, id, s, false).await?;
            sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3)").bind(t).bind(format!("app.{app}.source_imported")).bind(json!({"sourceId":id,"app":app,"sourceKind":kind,"title":title,"metadata":s["metadata"]})).execute(&mut *tx).await?;
        }
    }
    sqlx::query("INSERT INTO app_export_cursors(tenant,app,cursor) VALUES($1,$2,$3) ON CONFLICT(tenant,app) DO UPDATE SET cursor=greatest(app_export_cursors.cursor,EXCLUDED.cursor)").bind(t).bind(app).bind(cursor).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub(crate) async fn private_evidence(a: &App, t: &str, query: &str) -> Result<Value> {
    let rows=sqlx::query("SELECT e.* FROM app_evidence e JOIN app_packages p ON p.tenant=e.tenant AND p.id=e.app AND p.active WHERE e.tenant=$1 ORDER BY ts_rank_cd(to_tsvector('simple',e.title||' '||e.body),plainto_tsquery('simple',$2)) DESC,e.updated_at DESC LIMIT 12").bind(t).bind(query).fetch_all(&a.db).await?;
    Ok(json!(rows.iter().map(|r|json!({"app":r.get::<String,_>("app"),"sourceId":r.get::<String,_>("source_id"),"title":r.get::<String,_>("title"),"text":r.get::<String,_>("body").chars().take(2500).collect::<String>(),"sourceUrl":r.get::<String,_>("source_url"),"metadata":r.get::<Value,_>("metadata"),"digest":r.get::<String,_>("digest"),"visibility":"merchant-private","trustedInstructions":false})).collect::<Vec<_>>()))
}
pub(crate) async fn purge_sources(a: &App, t: &str, app: &str, cursor: i64) -> Result<()> {
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,39))")
        .bind(t)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app_evidence WHERE tenant=$1 AND app=$2")
        .bind(t)
        .bind(app)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO app_export_cursors(tenant,app,cursor) VALUES($1,$2,$3) ON CONFLICT(tenant,app) DO UPDATE SET cursor=EXCLUDED.cursor")
        .bind(t)
        .bind(app)
        .bind(cursor)
        .execute(&mut *tx)
        .await?;
    knowledge::sync_integration(&mut tx, t, app, "", &json!({}), true).await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn private_graph(a: &App, t: &str) -> Result<Value> {
    let active: Vec<String> =
        sqlx::query_scalar("SELECT id FROM app_packages WHERE tenant=$1 AND active")
            .bind(t)
            .fetch_all(&a.db)
            .await?;
    let sources=knowledge::cypher(&a.db,"MATCH (s:Shop {tenant:$tenant})-[:HAS_PRIVATE_SOURCE]->(e:AppEvidence {tenant:$tenant}) WHERE e.app IN $active RETURN {app:e.app,sourceId:e.source_id,title:e.title,kind:e.kind} LIMIT 100",json!({"tenant":t,"active":active})).await?;
    let products=knowledge::cypher(&a.db,"MATCH (e:AppEvidence {tenant:$tenant})-[:REFERENCES_PRODUCT]->(p:Product {tenant:$tenant}) WHERE e.app IN $active RETURN {app:e.app,sourceId:e.source_id,productId:p.product_id} LIMIT 100",json!({"tenant":t,"active":active})).await?;
    Ok(json!({"sources":sources,"productRelations":products,"visibility":"merchant-private"}))
}
