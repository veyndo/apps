//! Capture accepted definitions in existing operational JSON under native package locks; no app-specific tables.
use super::*;
async fn packages(conn: &mut sqlx::PgConnection, t: &str) -> Result<Vec<Manifest>> {
    let rows: Vec<Value> = sqlx::query_scalar("SELECT manifest FROM app_packages WHERE tenant=$1 AND active AND jsonb_array_length(coalesce(manifest->'lifecycles','[]'))>0 ORDER BY id LIMIT 9 FOR SHARE")
        .bind(t).fetch_all(conn).await?;
    if rows.len() > 8 {
        return Err(bad("At most eight native lifecycle apps per shop"));
    }
    rows.into_iter()
        .map(|v| serde_json::from_value(v).map_err(|_| bad("Invalid installed lifecycle")))
        .collect()
}
/// Called under the existing aggregate configuration lock, including selected publication.
pub(crate) async fn validate_active_locale(
    conn: &mut sqlx::PgConnection,
    t: &str,
    main: &str,
) -> Result<()> {
    for m in packages(conn, t).await? {
        validate_locale(&m, main)?;
    }
    Ok(())
}
pub(crate) async fn admission(conn: &mut sqlx::PgConnection, t: &str, m: &Manifest) -> Result<()> {
    if m.lifecycles.is_empty() {
        return Ok(());
    }
    let main: Option<String> = sqlx::query_scalar(
        "SELECT coalesce(data->>'mainLocale','en-GB') FROM commerce_settings WHERE tenant=$1",
    )
    .bind(t)
    .fetch_optional(&mut *conn)
    .await?;
    validate_locale(m, main.as_deref().unwrap_or("en-GB"))?;
    let mut apps = packages(conn, t).await?;
    apps.retain(|old| old.id != m.id);
    apps.push(m.clone());
    if apps.len() > 8 {
        return Err(bad("At most eight native lifecycle apps per shop"));
    }
    let (base, _) = commerce::machine_conn(conn, t).await?;
    compile(base, &apps, "order")?;
    compile(quote_machine(), &apps, "quote")?;
    Ok(())
}
pub(crate) async fn capture(conn: &mut sqlx::PgConnection, t: &str, object: &str) -> Result<Value> {
    // This single ordered package snapshot is held until the native object commits.
    let apps = packages(conn, t).await?;
    if !apps
        .iter()
        .any(|m| m.lifecycles.iter().any(|c| c.object == object))
    {
        return Ok(Value::Null);
    }
    let (base, revision) = if object == "order" {
        commerce::machine_conn(conn, t).await?
    } else {
        (quote_machine(), 0)
    };
    let machine = compile(base, &apps, object)?;
    let main_locale: Option<String> =
        sqlx::query_scalar("SELECT data->>'mainLocale' FROM commerce_settings WHERE tenant=$1")
            .bind(t)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
    let main_locale = main_locale.unwrap_or_else(|| "en-GB".into());
    Ok(json!(Snapshot {
        version: 1,
        object: object.into(),
        revision,
        main_locale,
        machine
    }))
}
pub(crate) fn snapshot(data: &Value, object: &str) -> Result<Option<Snapshot>> {
    let Some(value) = data.get("nativeLifecycle") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let s: Snapshot = serde_json::from_value(value.clone())
        .map_err(|_| conflict("Invalid accepted lifecycle; manual recovery required"))?;
    if s.version != 1 || s.object != object {
        return Err(conflict("Accepted lifecycle version mismatch"));
    }
    Ok(Some(s))
}
pub(crate) fn order_machine(
    data: &Value,
    fallback: commerce::OrderMachine,
    revision: i64,
) -> Result<(commerce::OrderMachine, i64)> {
    Ok(snapshot(data, "order")?
        .map(|s| (s.machine, s.revision))
        .unwrap_or((fallback, revision)))
}

pub(crate) async fn validate_order_base(
    conn: &mut sqlx::PgConnection,
    t: &str,
    base: commerce::OrderMachine,
) -> Result<()> {
    let apps = packages(conn, t).await?;
    compile(base, &apps, "order")?;
    Ok(())
}

pub(crate) async fn current_order(db: &PgPool, t: &str) -> Result<commerce::OrderMachine> {
    let mut conn = db.begin().await?;
    let apps = packages(&mut conn, t).await?;
    let (base, _) = commerce::machine_conn(&mut conn, t).await?;
    compile(base, &apps, "order")
}
/// Only declared display labels cross the customer projection; never app policies or grants.
pub(crate) fn state_labels(raw: &Value) -> Option<(Value, Value)> {
    snapshot(raw, "order").ok().flatten().map(|s| {
        (
            json!(
                s.machine
                    .states
                    .iter()
                    .map(|s| (&s.id, &s.label))
                    .collect::<HashMap<_, _>>()
            ),
            json!(s.main_locale),
        )
    })
}
pub(crate) fn project_labels(raw: &Value, output: &mut Value) {
    if let Some((labels, main)) = state_labels(raw) {
        output["stateLabels"] = labels;
        output["stateMainLocale"] = main;
    }
}

pub(crate) async fn bind_order(
    conn: &mut sqlx::PgConnection,
    t: &str,
    order: &mut Value,
) -> Result<()> {
    let lifecycle = capture(conn, t, "order").await?;
    if !lifecycle.is_null() {
        order["nativeLifecycle"] = lifecycle;
    }
    Ok(())
}
