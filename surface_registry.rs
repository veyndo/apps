//! Current-authority app surface projection shared by storefronts and native visual composition.
use super::*;
pub(crate) async fn registry(a: &App, h: &RequestContext, public: bool) -> Result<Value> {
    if !public {
        merchant(a, h)?;
    }
    let (settings, _) = commerce::config(a, &tenant(h)?).await?;
    let rows =
        sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active ORDER BY id")
            .bind(tenant(h)?)
            .fetch_all(&a.db)
            .await?;
    let mut surfaces = vec![];
    let mut public_apps = vec![];
    for row in rows {
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid app package"))?;
        if m.actions.iter().any(|act| act.public) {
            public_apps.push(m.id.clone());
        }
        for s in &m.surfaces {
            if s.location.starts_with("admin.") == public
                || s.permission
                    .as_deref()
                    .is_some_and(|p| auth::permit(h, p).is_err())
            {
                continue;
            }
            if let Some(mut view) = native_views::payload(&m, s) {
                let allowed = s
                    .actions
                    .iter()
                    .filter(|name| {
                        m.actions
                            .iter()
                            .any(|act| act.name == **name && gateway::action_authorized(a, h, act))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if let Some(blocks) = view["view"]["blocks"].as_array_mut() {
                    blocks.retain(|b| {
                        ["text", "button", "frame", "tabs"]
                            .iter()
                            .any(|kind| b["kind"] == *kind)
                            || b["readAction"]
                                .as_str()
                                .is_some_and(|n| allowed.iter().any(|a| a == n))
                    });
                    for b in blocks {
                        if b["kind"] == "form"
                            && b["writeAction"]
                                .as_str()
                                .is_none_or(|n| !allowed.iter().any(|a| a == n))
                        {
                            b["kind"] = json!("cards");
                            b.as_object_mut().unwrap().remove("writeAction");
                        }
                    }
                }
                view["tenant"] = json!(tenant(h)?);
                let mut surface = json!(s);
                surface["actions"] = json!(allowed);
                surfaces.push(json!({"app":m.id,"version":m.version,"surface":surface,"native":view,"readOnlyActions":s.actions.iter().filter(|name|m.actions.iter().any(|act|act.name==**name && gateway::action_authorized(a,h,act) && super::surfaces::readonly(act))).collect::<Vec<_>>(),"mainLocale":settings.main_locale,"locales":settings.locales}));
            } else if approval::approved(&m, &crate::runtime_config::get().services[&m.id])
                && let Some(url) = super::surfaces::surface_url(&m.id, &s.ui_path)
            {
                surfaces.push(json!({"app":m.id,"version":m.version,"surface":s,"url":url,"mainLocale":settings.main_locale,"locales":settings.locales,"readOnlyActions":s.actions.iter().filter(|name|m.actions.iter().any(|act|act.name==**name && gateway::action_authorized(a,h,act) && super::surfaces::readonly(act))).collect::<Vec<_>>()}));
            }
        }
    }
    Ok(json!({"surfaces":surfaces,"publicApps":public_apps,"mainLocale":settings.main_locale}))
}
