//! Persist hosted-app dependencies through the existing package installer; no private renderer or parallel registry.
use super::*;
use sqlx::{Postgres, Transaction};
pub(crate) const STORYFRONT: &str = include_str!("../../extensions/apps/storyfront/manifest.json");

pub(crate) async fn ensure(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    id: &str,
) -> Result<()> {
    if id != "storyfront" {
        return Err(bad("Unknown hosted app"));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7))")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    let active: Option<bool> =
        sqlx::query_scalar("SELECT active FROM app_packages WHERE tenant=$1 AND id=$2 FOR UPDATE")
            .bind(tenant)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    match active {
        None => {
            install_tx(
                tx,
                tenant,
                serde_json::from_str(STORYFRONT).map_err(|_| bad("Invalid built-in"))?,
            )
            .await?;
        }
        Some(false) => {
            sqlx::query(
                "UPDATE app_packages SET active=true,revision=revision+1 WHERE tenant=$1 AND id=$2",
            )
            .bind(tenant)
            .bind(id)
            .execute(&mut **tx)
            .await?;
            sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.activated',$2)")
                .bind(tenant)
                .bind(json!({"app":id,"reason":"hosted_frontend"}))
                .execute(&mut **tx)
                .await?;
        }
        Some(true) => {}
    }
    Ok(())
}

/// Migration 055 runs once under the migration lock; normal reads never install apps.
pub(crate) async fn backfill(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    let tenants: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT tenant FROM hosted_frontends WHERE app_id='storyfront' ORDER BY tenant",
    )
    .fetch_all(&mut **tx)
    .await?;
    for tenant in tenants {
        ensure(tx, &tenant, "storyfront").await?;
    }
    sqlx::query("ALTER TABLE hosted_frontends VALIDATE CONSTRAINT hosted_frontends_app_fk")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn connections(a: &App, tenant: &str) -> Result<Vec<Value>> {
    let rows=sqlx::query("SELECT alias,channel,experience_alias,revision,app_id FROM hosted_frontends WHERE tenant=$1 ORDER BY alias")
        .bind(tenant).fetch_all(&a.db).await?;
    Ok(rows.iter().map(|r| {
        let alias:String=r.get("alias"); let experience:String=r.get("experience_alias");
        json!({"alias":alias,"channel":r.get::<String,_>("channel"),"experienceAlias":experience,"revision":r.get::<i64,_>("revision"),"appId":r.get::<Option<String>,_>("app_id"),"url":crate::shop_domains::links(&alias)["storefrontUrl"],"editorUrl":crate::shop_domains::frontend_editor::url(&experience,env::var("HOSTED_FRONTEND_EDITOR_URL").ok().as_deref())})
    }).collect())
}

/// Existing action contract resolves managed connections locally, without inventing a second generator.
pub(crate) async fn action(a: &App, tenant: &str, id: &str, name: &str) -> Result<Option<Value>> {
    if id != "storyfront" {
        return Ok(None);
    }
    let bindings: Vec<Value> = connections(a, tenant)
        .await?
        .into_iter()
        .filter(|f| f["appId"] == id)
        .collect();
    if bindings.is_empty() {
        return Ok(None);
    }
    if name == "status" {
        return Ok(Some(
            json!({"managedBy":"experience","frontends":bindings,"status":"connected"}),
        ));
    }
    if name == "generate" {
        return Err(conflict(
            "Use the connected Storyfront editor to generate and publish this Experience",
        ));
    }
    Ok(None)
}
