//! Managed portable record writes: current authority, installed package fence, full field validation, CAS/history/outbox and one commit.
use crate::{
    App,
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
    portable,
    write_contract::{History, Write},
};
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::Row;
use veyndo_app_storage::{
    self as store, managed_hooks,
    model::{self, admission::ScopeAuthority, manifest::Manifest},
    mutations::{Mutation, WriteError},
};
fn bad() -> Problem {
    Problem::new(
        StatusCode::BAD_REQUEST,
        "invalid_write",
        "Provide the approved package-bound record input with its current revision",
    )
}
fn conflict() -> Problem {
    Problem::new(
        StatusCode::CONFLICT,
        "package_changed",
        "The installed App package changed; reopen and review it",
    )
}
fn enabled(app: &App) -> Result<()> {
    if app.config.writes {
        Ok(())
    } else {
        Err(Problem::new(
            StatusCode::FORBIDDEN,
            "write_disabled",
            "This host is configured for reads only",
        ))
    }
}
fn identifier(s: &str) -> Result<()> {
    if s.is_empty() || !veyndo_app_authority::database::identifier(s) {
        Err(bad())
    } else {
        Ok(())
    }
}
fn admit(
    p: &Principal,
    m: &Manifest,
    name: &str,
    value: &Value,
    handler: &str,
    mcp: bool,
) -> Result<model::Entity> {
    let action = m
        .actions
        .iter()
        .find(|a| a.name == name && a.handler == handler && !a.public)
        .ok_or(Problem::new(
            StatusCode::NOT_FOUND,
            "action_unavailable",
            "The private installed action is unavailable",
        ))?;
    if mcp && action.mcp != Some(true) {
        return Err(Problem::new(
            StatusCode::FORBIDDEN,
            "mcp_denied",
            "This App action is not enabled for MCP",
        ));
    }
    model::admission::check(p, action, value).map_err(|e| match e {
        model::admission::AdmissionError::Authority(e) => e,
        _ => bad(),
    })?;
    m.entities
        .iter()
        .find(|e| Some(&e.name) == action.entity.as_ref())
        .cloned()
        .ok_or_else(unavailable)
}
fn error(e: WriteError<managed_hooks::Error>) -> Problem {
    match e {
        WriteError::Revision(_) => Problem::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            "The App record revision changed; reload it before saving",
        ),
        WriteError::Conflict(_) => Problem::new(
            StatusCode::CONFLICT,
            "write_conflict",
            "The record cannot be changed from its current state",
        ),
        WriteError::Invalid(_) => bad(),
        WriteError::Hook(managed_hooks::Error::Model(e)) => Problem::new(
            if e.kind == model::Kind::Conflict {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            },
            "workflow_denied",
            "The record state or immutable fields do not allow this change",
        ),
        _ => unavailable(),
    }
}
pub async fn write(app: &App, p: Principal, input: Write, mcp: bool) -> Result<Value> {
    enabled(app)?;
    if !input.approve
        || input.input.revision < 0
        || input.input.id.is_empty()
        || input.input.id.len() > 100
        || input.package_digest.len() != 64
        || !input
            .package_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(bad());
    }
    p.require_scope("apps.manage")?;
    identifier(&input.app)?;
    identifier(&input.action)?;
    let raw = serde_json::to_value(&input).map_err(|_| bad())?;
    model::schema::validate(&crate::write_contract::schema(), &raw).map_err(|_| bad())?;
    let _slot = app.execution.clone().try_acquire_owned().map_err(|_| {
        Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "data_capacity",
            "The data service is busy; retry shortly",
        )
    })?;
    let mut tx = app.begin_write(&p).await?;
    let scope = store::Scope {
        tenant: &p.tenant,
        app: &input.app,
    };
    let raw = store::packages::lock(&mut tx, scope)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(conflict)?;
    if store::packages::digest(&raw) != input.package_digest {
        return Err(conflict());
    }
    let manifest: Manifest = serde_json::from_value(raw).map_err(|_| unavailable())?;
    if manifest.id != input.app || !manifest.permissions.iter().any(|s| s == "data.write") {
        return Err(conflict());
    }
    if input.input.fields.to_string().len() > 65536 {
        return Err(bad());
    }
    let value = serde_json::to_value(&input.input).map_err(|_| bad())?;
    let entity = admit(&p, &manifest, &input.action, &value, "save", mcp)?;
    portable::check(&manifest, &entity)?;
    model::fields::validate(&entity, &input.input.fields, &portable::Policy).map_err(|_| {
        Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid_fields",
            "Fields do not match the installed model",
        )
    })?;
    let names = entity
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>();
    let key = store::RecordKey {
        scope,
        entity: &entity.name,
        id: &input.input.id,
    };
    let result = store::mutations::write(
        &mut tx,
        &Mutation {
            key,
            expected_revision: input.input.revision,
            fields: &input.input.fields,
            declared_fields: &names,
            replay: false,
        },
        &managed_hooks::Hooks {
            key,
            manifest: &manifest,
            entity: &entity,
        },
    )
    .await
    .map_err(error)?;
    let action = manifest
        .actions
        .iter()
        .find(|a| a.name == input.action)
        .ok_or_else(unavailable)?;
    if let Some(schema) = &action.output_schema {
        model::schema::validate(schema, &result).map_err(|_| {
            Problem::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "output_contract",
                "The result does not match the installed action output contract",
            )
        })?;
    }
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(result)
}
impl store::record_history::ReadAuthority for Principal {
    fn allowed(&self, scope: &str) -> bool {
        self.permits(scope)
    }
}
pub async fn history(app: &App, p: Principal, input: History, mcp: bool) -> Result<Value> {
    enabled(app)?;
    identifier(&input.app)?;
    identifier(&input.action)?;
    if input.id.is_empty() || input.id.len() > 100 {
        return Err(bad());
    }
    let mut tx = app.begin(&p).await?;
    let raw: Option<Value> = sqlx::query_scalar(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active",
    )
    .bind(&p.tenant)
    .bind(&input.app)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    let manifest: Manifest =
        serde_json::from_value(raw.ok_or_else(conflict)?).map_err(|_| unavailable())?;
    if manifest.id != input.app || !manifest.permissions.iter().any(|s| s == "data.read") {
        return Err(conflict());
    }
    let entity = admit(
        &p,
        &manifest,
        &input.action,
        &json!({"id":input.id}),
        "list",
        mcp,
    )?;
    portable::check(&manifest, &entity)?;
    let key = store::RecordKey {
        scope: store::Scope {
            tenant: &p.tenant,
            app: &input.app,
        },
        entity: &entity.name,
        id: &input.id,
    };
    let rows = store::record_history::list(&mut tx, key, &p)
        .await
        .map_err(|_| unavailable())?;
    let versions=rows.iter().map(|r| {
  let fields:Value=r.get("fields");
  json!({"revision":r.get::<i64,_>("revision"),"fields":entity.fields.iter().map(|f|(f.name.clone(),fields[&f.name].clone())).collect::<serde_json::Map<_,_>>(),"actor":r.get::<Option<String>,_>("actor"),"source":r.get::<String,_>("source"),"createdAt":r.get::<String,_>("created_at")})
 }).collect::<Vec<_>>();
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(json!({"versions":versions}))
}
