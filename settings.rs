//! Declared app settings reuse the digest-bound encrypted secret owner; secret values never have a browser read path.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Setting {
    pub key: String,
    pub label: HashMap<String, String>,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    if m.settings.len() > 16 {
        return Err(bad("Maximum 16 app settings"));
    }
    for f in &m.settings {
        if !identifier(&f.key)
            || !names.insert(&f.key)
            || f.label.is_empty()
            || f.label.len() > 100
            || f.label.values().any(|s| s.is_empty() || s.len() > 100)
            || !["string", "secret", "boolean", "integer"].contains(&f.kind.as_str())
        {
            return Err(bad("Invalid app setting declaration"));
        }
    }
    Ok(())
}
pub(super) fn router() -> Router<App> {
    Router::new().secure_route(
        "/api/apps/{id}/settings",
        &[("GET", "apps.manage"), ("PUT", "apps.manage")],
        get(read).put(save),
    )
}
async fn read(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    auth::permit(&h, "team.manage")?;
    if !verified_kernel::persistent_key_admissible(
        h.principal.personal_session,
        auth::permit(&h, "team.manage").is_ok(),
    ) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Personal administrator session required".into(),
        ));
    }
    let m = package(&a, &t, &id, false).await?;
    let digest = approval::canonical_digest(&m);
    let row=sqlx::query("SELECT revision,package_digest FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind='settings'").bind(&t).bind(&id).fetch_optional(&a.db).await?;
    let revision = row
        .as_ref()
        .map(|r| r.get::<i64, _>("revision"))
        .unwrap_or(0);
    let current = row
        .as_ref()
        .is_none_or(|r| r.get::<String, _>("package_digest") == digest);
    let values = if current {
        resolve(&a, &t, &m).await?
    } else {
        json!({})
    };
    let visible: serde_json::Map<String, Value> = m
        .settings
        .iter()
        .filter(|f| f.kind != "secret")
        .filter_map(|f| values.get(&f.key).map(|v| (f.key.clone(), v.clone())))
        .collect();
    let configured = m
        .settings
        .iter()
        .filter(|f| values.get(&f.key).is_some_and(|v| !v.is_null()))
        .map(|f| f.key.clone())
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"fields":m.settings,"values":visible,"configured":configured,"revision":revision,"digest":digest,"current":current,"canManage":staging::parent(&a,&t).await?.is_none(),"encryptionReady":veyndo::inference::settings::encryption_ready()}),
    ))
}
async fn save(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    auth::permit(&h, "team.manage")?;
    if !verified_kernel::persistent_key_admissible(
        h.principal.personal_session,
        auth::permit(&h, "team.manage").is_ok(),
    ) || staging::parent(&a, &t).await?.is_some()
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Settings require a personal live-shop administrator session".into(),
        ));
    }
    let m = package(&a, &t, &id, true).await?;
    let digest = approval::canonical_digest(&m);
    if v["approve"] != true || v["digest"] != digest {
        return Err(conflict(
            "Approve the current package before saving settings",
        ));
    }
    let revision = v["revision"]
        .as_i64()
        .filter(|n| *n >= 0)
        .ok_or(bad("Settings revision required"))?;
    let mut values=if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind='settings' AND package_digest=$3)").bind(&t).bind(&id).bind(&digest).fetch_one(&a.db).await? {resolve(&a,&t,&m).await?} else {json!({})};
    let input = v["values"]
        .as_object()
        .filter(|vs| vs.len() <= 16)
        .ok_or(bad("Settings object required"))?;
    for (key, value) in input {
        let f = m
            .settings
            .iter()
            .find(|f| f.key == *key)
            .ok_or(bad("Undeclared app setting"))?;
        let valid = match f.kind.as_str() {
            "boolean" => value.is_boolean(),
            "integer" => value.as_i64().is_some(),
            _ => value
                .as_str()
                .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control)),
        };
        if !valid && !value.is_null() {
            return Err(bad("Invalid app setting value"));
        }
        values[key] = value.clone();
    }
    for f in &m.settings {
        if f.required
            && values
                .get(&f.key)
                .is_none_or(|v| v.is_null() || v.as_str().is_some_and(str::is_empty))
        {
            return Err(bad(format!("Required app setting: {}", f.key)));
        }
    }
    let encoded = serde_json::to_string(&values).map_err(|_| bad("Invalid settings"))?;
    if encoded.len() > 16384 {
        return Err(bad("Settings exceed 16 KiB"));
    }
    let revision = secrets::store(
        &a,
        &t,
        &id,
        secrets::SecretWrite {
            kind: "settings",
            digest: &digest,
            revision,
            actor: h
                .principal
                .user
                .as_deref()
                .ok_or(bad("Personal account required"))?,
            secret: &encoded,
        },
    )
    .await?;
    Ok(Json(json!({"saved":true,"revision":revision})))
}
pub(super) async fn resolve(a: &App, t: &str, m: &Manifest) -> Result<Value> {
    let Some(encoded) = secrets::resolve(a, t, m, "settings").await? else {
        return Ok(json!({}));
    };
    let values: Value =
        serde_json::from_str(&encoded).map_err(|_| bad("Invalid encrypted app settings"))?;
    // A removed declaration never becomes available to a new package or remote service.
    Ok(Value::Object(
        m.settings
            .iter()
            .filter_map(|f| values.get(&f.key).map(|v| (f.key.clone(), v.clone())))
            .collect(),
    ))
}
