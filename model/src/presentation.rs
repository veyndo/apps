//! Optional passive app artwork and localized summaries; omitted metadata preserves published legacy digests.
use crate::{Result, bad};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Presentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub description: HashMap<String, String>,
}
pub fn validate(p: Option<&Presentation>) -> Result<()> {
    let Some(p) = p else {
        return Ok(());
    };
    for value in [&p.icon, &p.cover].into_iter().flatten() {
        let relative = (value.starts_with("/media/") || value.starts_with("/assets/"))
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
            && !value.contains("..");
        let remote = url::Url::parse(value).is_ok_and(|u| {
            u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        });
        if value.len() > 2048 || (!relative && !remote) {
            return Err(bad(
                "App artwork requires a media/assets path or credential-free HTTPS URL",
            ));
        }
    }
    if p.description.len() > 100
        || p.description.iter().any(|(l, s)| {
            l.is_empty()
                || l.len() > 35
                || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || s.len() > 1200
        })
    {
        return Err(bad("Invalid localized app summary"));
    }
    Ok(())
}
