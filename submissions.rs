//! Public append-only intake uses declared schemas, existing cart admission, quotas, records and outbox.
use super::*;
pub(super) async fn submit(
    a: &App,
    h: &RequestContext,
    m: &Manifest,
    e: &Entity,
    v: &Value,
) -> Result<Value> {
    let t = tenant(h)?;
    let live = crate::staging::parent(a, &t).await?.is_none();
    let cart = load_cart(a, h).await?;
    if !verified_kernel::app_submission_admissible(
        !e.public_read,
        live,
        cart.tenant == t && cart.data.sales_channel == marketing::channel_id(h),
    ) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Submissions require a private live inbox and matching cart".into(),
        ));
    }
    let intake_key = hash(&cart.id);
    let id = format!("submission_{}", uuid::Uuid::new_v4().simple());
    let mut tx = a.db.begin().await?;
    sqlx::query(
        "SELECT set_config('veyndo.source','app.submit',true),set_config('veyndo.actor','',true)",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,946))")
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM outbox WHERE tenant=$1 AND kind=$2 AND created_at>now()-interval '1 minute'").bind(&t).bind(format!("app.{}.submitted",m.id)).fetch_one(&mut *tx).await?;
    let personal:i64=sqlx::query_scalar("SELECT count(*) FROM outbox WHERE tenant=$1 AND kind=$2 AND data->>'intakeKey'=$3 AND created_at>now()-interval '1 minute'").bind(&t).bind(format!("app.{}.submitted",m.id)).bind(&intake_key).fetch_one(&mut *tx).await?;
    if count >= 60 || personal >= 5 {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Please wait a minute before submitting another request".into(),
        ));
    }
    if m.id == "site_forms" {
        site_forms::admit(&mut tx, &t, marketing::channel_id(h), v).await?;
    }
    let _ = data::save_tx(
        &mut tx,
        &t,
        m,
        e,
        &json!({"id":id,"revision":0,"fields":v["fields"]}),
    )
    .await?;
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3)")
        .bind(&t)
        .bind(format!("app.{}.submitted", m.id))
        .bind(json!({"app":m.id,"entity":e.name,"id":id,"intakeKey":intake_key}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({"accepted":true}))
}
