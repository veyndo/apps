//! Reviewed job records use current creator rights and a locked live package; private targets never become app credentials.
use super::*;
// Retain a connection for the native owner even on the supported two-connection pool.
// This is process admission, not a claim of fleet-wide throughput isolation.
static IMPORT_WRITE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
pub(super) async fn execute(
    a: &App,
    h: &RequestContext,
    m: &Manifest,
    v: &Value,
    inspect: bool,
) -> Result<Value> {
    let mut metadata = v.clone();
    metadata
        .as_object_mut()
        .ok_or(bad("Import object required"))?
        .remove("assetChunks");
    input_schema::validate_native(
        &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"lease":{"type":"string","maxLength":100},"record":{"type":"object"}},"required":["id","lease","record"],"additionalProperties":false}),
        &metadata,
    )?;
    let live = merchant(a, h)?;
    if staging::parent(a, &live).await?.is_some() {
        return Err(bad("Job callback requires its live workspace"));
    }
    let kind = v["record"]["kind"]
        .as_str()
        .ok_or(bad("Import kind required"))?;
    let capability = match kind {
        "products" | "categories" => "products.write",
        "assets" => "assets.write",
        "reviews" => "reviews.write",
        "coupons" => "promotions.write",
        "pages" => "data.write",
        "customers" => "customers.write",
        "orders" => "orders.import",
        _ => return Err(bad("Native job import domain unavailable")),
    };
    let read_capability = match kind {
        "products" | "categories" | "reviews" => "products.read",
        "assets" => "assets.read",
        "pages" => "data.read",
        "customers" => "customers.read",
        "orders" => "orders.read",
        "coupons" => "promotions.write",
        _ => unreachable!(),
    };
    for p in ["jobs.write", capability]
        .into_iter()
        .chain(inspect.then_some(read_capability))
    {
        if !m.permissions.iter().any(|s| s == p) {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Import capability was not approved".into(),
            ));
        }
        credentials::permit(h, p)?;
        auth::permit(h, credentials::scope(p).unwrap())?;
    }
    if ["customers", "orders", "reviews"].contains(&kind) {
        if !m.permissions.iter().any(|p| p == "customers.pii") {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Historical personal data needs explicit PII consent".into(),
            ));
        }
        credentials::permit(h, "customers.pii")?;
        auth::permit(h, "customers.pii")?;
    }
    let _reserve = tokio::time::timeout(std::time::Duration::from_secs(10), IMPORT_WRITE.acquire())
        .await
        .map_err(|_| {
            Error(
                StatusCode::TOO_MANY_REQUESTS,
                "Native import capacity is busy; retry the reviewed job".into(),
            )
        })?
        .map_err(|_| bad("Native import admission unavailable"))?;
    // The native coordinator must read and lock an admitted child environment.
    // This bounded privileged transaction never leaves Core; every query binds live tenant/job/actor.
    let mut tx = a.db.begin_privileged().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&live)
        .execute(&mut *tx)
        .await?;
    let row=sqlx::query("SELECT actor,action,package_digest,input FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3 AND status='running' AND lease=$4 AND lease_until>now() AND now()<started_at+interval '30 minutes' FOR SHARE").bind(&live).bind(&m.id).bind(v["id"].as_str()).bind(v["lease"].as_str()).fetch_optional(&mut *tx).await?.ok_or(conflict("Job is cancelled, expired or has another lease"))?;
    if row.get::<String, _>("package_digest") != approval::canonical_digest(m) {
        return Err(conflict("Reviewed job package changed"));
    }
    let actor: String = row.get("actor");
    sqlx::query("SELECT m.user_id FROM memberships m JOIN workspace_roles r ON r.tenant=m.tenant AND r.id=m.role WHERE m.tenant=$1 AND m.user_id=$2 FOR SHARE OF m,r").bind(&live).bind(&actor).fetch_optional(&mut *tx).await?.ok_or(Error(StatusCode::FORBIDDEN,"Job creator no longer has access".into()))?;
    let grants = sqlx::query(
        "SELECT role,permissions FROM membership_grants WHERE tenant=$1 AND user_id=$2 AND active",
    )
    .bind(&live)
    .bind(&actor)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error(
        StatusCode::FORBIDDEN,
        "Job creator no longer has access".into(),
    ))?;
    let mut identity = RequestContext::new();
    identity.principal.user = Some(actor.clone());
    identity.principal.role = Some(grants.get("role"));
    let rights: Value = grants.get("permissions");
    identity.principal.permissions = (!rights.is_null()).then(|| rights.to_string());
    auth::permit(&identity, "apps.manage")?;
    auth::permit(&identity, credentials::scope(capability).unwrap())?;
    if inspect {
        auth::permit(&identity, credentials::scope(read_capability).unwrap())?;
    }
    if ["customers", "orders", "reviews"].contains(&kind) {
        auth::permit(&identity, "customers.pii")?;
    }
    let action = m
        .actions
        .iter()
        .find(|x| x.name == row.get::<String, _>("action") && x.handler == "job")
        .ok_or(conflict("Job action removed"))?;
    auth::permit(
        &identity,
        action.permission.as_deref().unwrap_or("apps.manage"),
    )?;
    gateway::additional_permissions(&identity, action)?;
    let intent: Value = row.get("input");
    if intent["operation"] == "reconcile" && !inspect {
        return Err(conflict(
            "Read-only reconciliation does not authorize native writes",
        ));
    }
    if intent["kind"] != kind
        || !intent["recordDigests"].as_array().is_some_and(|ds| {
            !ds.is_empty()
                && ds.len() <= 100
                && ds.iter().any(|d| *d == hash(&v["record"].to_string()))
        })
    {
        return Err(conflict(
            "Record differs from the explicitly reviewed job snapshot",
        ));
    }
    let target = intent["target"]
        .as_str()
        .ok_or(bad("Reviewed target required"))?;
    if ["products", "categories", "pages", "assets"].contains(&kind) {
        auth::permit(&identity, "environments.manage")?;
        sqlx::query("SELECT tenant FROM shop_environments WHERE tenant=$1 AND live_tenant=$2 AND preview_owner IS NULL FOR SHARE").bind(target).bind(&live).fetch_optional(&mut *tx).await?.ok_or(Error(StatusCode::FORBIDDEN,"Job target is not an admitted private environment".into()))?;
    } else if target != live {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Operational history belongs to the reviewed live workspace".into(),
        ));
    }
    lock_package_write(&mut tx, &live, m).await?;
    identity.insert(
        "x-tenant",
        target.parse().map_err(|_| bad("Invalid reviewed target"))?,
    );
    identity.principal.tenant = Some(target.into());
    let mut input = json!({"approve":true,"record":v["record"],"options":intent["options"]});
    if let Some(approved) = intent["options"]["acknowledgedWarnings"].as_array() {
        let warnings = v["record"]["warnings"]
            .as_array()
            .ok_or(bad("Invalid source warnings"))?;
        if approved.len() > 20
            || approved
                .iter()
                .any(|value| value.as_str().is_none_or(|s| s.len() > 2000))
            || warnings.iter().any(|warning| !approved.contains(warning))
        {
            return Err(bad(
                "Review every source limitation before importing this record",
            ));
        }
        if warnings.is_empty() {
            input["options"]
                .as_object_mut()
                .unwrap()
                .remove("acknowledgedWarnings");
        } else {
            input["options"]["acknowledgedWarnings"] = json!(warnings);
        }
    }
    if let Some(chunks) = v.get("assetChunks") {
        input["assetChunks"] = chunks.clone();
    }
    if inspect {
        input.as_object_mut().unwrap().remove("approve");
        input.as_object_mut().unwrap().remove("assetChunks");
    }
    // Native scope switch is limited to a job-locked target in this same admitted workspace.
    let operation = async {
        if inspect {
            crate::operations::imports::inspect(a, &identity, &input).await
        } else {
            crate::operations::imports::invoke(a, &identity, &input).await
        }
    };
    let result = veyndo::tenant_scope::scoped(
        veyndo::tenant_scope::Scope::Personal {
            tenant: target.into(),
            actor,
        },
        operation,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
