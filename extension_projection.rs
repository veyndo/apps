//! Bounded native-object read projection from existing app models, references and current package permissions.
use super::{Manifest, core_extensions::Object, editor_contract, gateway, records};
use crate::{App, RequestContext, Result, bad, merchant};
use serde_json::{Value, json};
use sqlx::Row;

pub(super) async fn attach(
    a: &App,
    h: &RequestContext,
    object: Object,
    rows: &mut [Value],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    if rows.len() > 100 {
        return Err(bad(
            "Extension projection supports at most 100 native objects",
        ));
    }
    let t = merchant(a, h)?;
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r["id"].as_str().map(str::to_owned))
        .collect();
    // CRM guest contacts have no immutable native customer ID and receive no extension binding.
    if ids.is_empty() {
        return Ok(());
    }
    let mut tx = a.db.begin().await?;
    let mut models = models(&mut tx, a, h, &t, object).await?;
    let omitted = models.len().saturating_sub(24);
    models.truncate(24);
    if models.is_empty() {
        tx.commit().await?;
        return Ok(());
    }
    let language: Value = sqlx::query_scalar("SELECT jsonb_build_object('mainLocale',data->'mainLocale','locales',data->'locales') FROM commerce_settings WHERE tenant=$1").bind(&t).fetch_one(&mut *tx).await?;
    let column = match object {
        Object::Product => "product_id",
        Object::Customer => "customer_id",
        Object::Order => "order_id",
        Object::Company => "company_id",
    };
    let allowed = json!(models.iter().map(|(app,_,e,_,_,_,_)|json!({"app":app,"entity":e.name,"field":e.extends.as_ref().unwrap().field})).collect::<Vec<_>>());
    // One indexed lookup for the entire page; SQL identifiers come exclusively from Object above.
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "WITH selected AS MATERIALIZED (SELECT r.app,r.entity,r.id,r.revision,r.fields,octet_length(r.fields::text) AS bytes,x.",
    );
    query.push(column).push(" AS native_id FROM app_record_references x JOIN app_records r ON r.tenant=x.tenant AND r.app=x.app AND r.entity=x.entity AND r.id=x.record_id JOIN jsonb_to_recordset(").push_bind(allowed).push("::jsonb) AS allowed(app text,entity text,field text) ON allowed.app=x.app AND allowed.entity=x.entity AND allowed.field=x.field WHERE x.tenant=").push_bind(&t).push(" AND x.").push(column).push("=ANY(").push_bind(&ids).push(") ) SELECT app,entity,id,revision,native_id,CASE WHEN bytes<=32768 AND sum(CASE WHEN bytes<=32768 THEN bytes ELSE 0 END) OVER(ORDER BY native_id,app,entity,id)<=524288 THEN fields ELSE NULL END AS fields FROM selected");
    let found = query.build().fetch_all(&mut *tx).await?;
    let found: std::collections::HashMap<(String, String, String), Value> = found
        .into_iter()
        .map(|r| {
            let mut v: Value = r.get::<Option<Value>, _>("fields").unwrap_or(Value::Null);
            if v.is_null() {
                return (
                    (r.get("app"), r.get("entity"), r.get("native_id")),
                    Value::Null,
                );
            }
            v["id"] = json!(r.get::<String, _>("id"));
            v["revision"] = json!(r.get::<i64, _>("revision"));
            ((r.get("app"), r.get("entity"), r.get("native_id")), v)
        })
        .collect();
    tx.commit().await?;
    for row in rows {
        let Some(id) = row["id"].as_str() else {
            continue;
        };
        let mut projections = Vec::new();
        let mut bytes = 0;
        let mut skipped = omitted;
        for (app, version, e, read, write, native, manifest) in &models {
            if found
                .get(&(app.clone(), e.name.clone(), id.to_owned()))
                .is_some_and(Value::is_null)
            {
                skipped += 1;
                continue;
            }
            let record = found
                .get(&(app.clone(), e.name.clone(), id.to_owned()))
                .map(|v| records::project(e, v));
            let ontology = record
                .as_ref()
                .and_then(|record| super::ontology::project(&t, manifest, e, &json!([record])));
            let out = json!({"app":app,"version":version,"entity":e.name,"label":e.label,"model":e,"readAction":read,"writeAction":write,"record":record,"language":language,"native":native,"ontology":ontology});
            let size = out.to_string().len();
            if bytes + size > 65536 {
                skipped += 1;
                continue;
            }
            bytes += size;
            projections.push(out);
        }
        row["extensions"] = json!(projections);
        row["omittedExtensions"] = json!(skipped);
    }
    Ok(())
}

pub(super) type Model = (
    String,
    String,
    super::Entity,
    String,
    Option<String>,
    Value,
    std::sync::Arc<Manifest>,
);
async fn models(
    tx: &mut sqlx::PgConnection,
    a: &App,
    h: &RequestContext,
    t: &str,
    object: Object,
) -> Result<Vec<Model>> {
    let packages = sqlx::query("SELECT id,manifest FROM app_packages WHERE tenant=$1 AND active AND EXISTS(SELECT 1 FROM app_models m WHERE m.tenant=app_packages.tenant AND m.app=app_packages.id AND m.definition->'extends'->>'object'=$2) ORDER BY id FOR SHARE")
        .bind(t).bind(json!(object).as_str().unwrap()).fetch_all(&mut *tx).await?;
    let mut models = Vec::new();
    for p in packages {
        let m: Manifest = serde_json::from_value(p.get("manifest"))
            .map_err(|_| bad("Invalid active extension package"))?;
        let m = std::sync::Arc::new(m);
        for e in &m.entities {
            if !e.extends.as_ref().is_some_and(|x| x.object == object)
                || editor_contract::entity_access(&m, e, h, "list").is_err()
            {
                continue;
            }
            let Some(read) = m.actions.iter().find(|action| {
                action.entity.as_deref() == Some(e.name.as_str())
                    && action.handler == "list"
                    && gateway::action_authorized(a, h, action)
            }) else {
                continue;
            };
            let write = m.actions.iter().find(|action| {
                action.entity.as_deref() == Some(e.name.as_str())
                    && action.handler == "save"
                    && gateway::action_authorized(a, h, action)
            });
            let lookups: serde_json::Map<String, Value> = m
                .actions
                .iter()
                .filter(|action| {
                    action.handler == "list" && gateway::action_authorized(a, h, action)
                })
                .filter_map(|action| {
                    action
                        .entity
                        .as_ref()
                        .map(|entity| (entity.clone(), json!(action.name)))
                })
                .collect();
            let assets: serde_json::Map<String, Value> = m
                .actions
                .iter()
                .filter(|action| {
                    matches!(
                        action.handler.as_str(),
                        "assets" | "asset_preview" | "asset_upload"
                    ) && gateway::action_authorized(a, h, action)
                })
                .map(|action| (action.handler.clone(), json!(action.name)))
                .collect();
            models.push((
                m.id.clone(),
                m.version.clone(),
                e.clone(),
                read.name.clone(),
                write.map(|a| a.name.clone()),
                json!({"lookupActions":lookups,"assetActions":assets}),
                m.clone(),
            ));
        }
    }
    Ok(models)
}

/// Planning receives only current readable field definitions, never records, secrets or native identities.
pub(super) async fn descriptions(a: &App, h: &RequestContext, object: Object) -> Result<Value> {
    let t = merchant(a, h)?;
    let mut tx = a.db.begin().await?;
    let models = models(&mut tx, a, h, &t, object).await?;
    let fields=bounded_fields(models.iter().flat_map(|(app,_,e,_,_,_,_)|e.fields.iter().filter(|f|f.indexed && !f.translatable && matches!(f.kind.as_str(),"string"|"integer"|"boolean") && Some(f.name.as_str())!=e.extends.as_ref().map(|x|x.field.as_str())).map(move |f|serde_json::json!({"app":app,"entity":e.name,"field":f.name,"kind":f.kind,"label":f.label}))));
    tx.commit().await?;
    Ok(serde_json::json!(fields))
}

// Keep complete definitions rather than truncating untrusted labels into a different instruction.
fn bounded_fields(fields: impl IntoIterator<Item = Value>) -> Vec<Value> {
    let mut bytes = 2;
    fields
        .into_iter()
        .filter(|field| {
            let size = field.to_string().len() + 1;
            if bytes + size > 16384 {
                return false;
            }
            bytes += size;
            true
        })
        .take(128)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn planning_metadata_is_bounded_without_rewriting_labels_or_starving_small_definitions() {
        let small =
            json!({"app":"example","entity":"details","field":"segment","label":{"en":"Segment"}});
        let large = json!({"label":{"en":"x".repeat(20000)}});
        let bounded = bounded_fields(vec![large, small.clone()]);
        assert_eq!(bounded, vec![small.clone()]);
        assert!(json!(bounded).to_string().len() <= 16384);
        assert_eq!(bounded_fields(vec![small; 200]).len(), 128);
    }
}
