//! Optional passive app artwork and localized summaries; omitted metadata preserves published legacy digests.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Presentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub description: HashMap<String, String>,
}
pub(crate) fn validate(p: Option<&Presentation>) -> Result<()> {
    let Some(p) = p else {
        return Ok(());
    };
    for value in [&p.icon, &p.cover].into_iter().flatten() {
        let relative = (value.starts_with("/media/") || value.starts_with("/assets/"))
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
            && !value.contains("..");
        let remote = reqwest::Url::parse(value).is_ok_and(|u| {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passive_artwork_validates_and_roundtrips_without_affecting_old_packages() {
        let m: Manifest = serde_json::from_str(include_str!(
            "../../extensions/apps/care-studio/manifest.json"
        ))
        .unwrap();
        assert!(json!(m).get("presentation").is_none());
        let mut generated = json!(m);
        generated["presentation"] =
            json!({"icon":null,"cover":null,"description":{"en":"Care guidance"}});
        let parsed: Manifest = serde_json::from_value(generated).unwrap();
        validate(parsed.presentation.as_ref()).unwrap();
        assert_eq!(
            json!(parsed)["presentation"],
            json!({"description":{"en":"Care guidance"}})
        );
        for url in [
            "/media/shop/cover.webp",
            "/assets/app.svg",
            "https://example.test/app.png",
        ] {
            let mut v = json!(m);
            v["presentation"] =
                json!({"icon":url,"cover":url,"description":{"es":"Guía de cuidado"}});
            let n: Manifest = serde_json::from_value(v).unwrap();
            validate(n.presentation.as_ref()).unwrap();
            assert_eq!(json!(n)["presentation"]["icon"], url);
        }
        for url in [
            "javascript:alert(1)",
            "data:image/svg+xml,x",
            "//example.test/x",
            "http://example.test/x",
            "https://user:secret@example.test/x",
            "/media/../secret",
            "file:///tmp/a",
        ] {
            let p = Presentation {
                icon: Some(url.into()),
                cover: None,
                description: HashMap::new(),
            };
            assert!(validate(Some(&p)).is_err(), "{url}");
        }
    }
}
