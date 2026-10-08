//! Signed publisher namespaces and tenant-local dependency/version gates. Operator service access still requires its separate exact digest pin.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Distribution {
    pub publisher: String,
    pub key_id: String,
    pub signature: String,
    pub channel: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Dependency {
    pub app: String,
    pub publisher: String,
    pub version: String,
}
/// Removing only the signature avoids self-reference. All permissions, bundles and dependencies are signed in the Rust canonical representation.
fn message(m: &Manifest) -> Vec<u8> {
    let mut m = m.clone();
    m.distribution.as_mut().unwrap().signature.clear();
    format!("vendune-package-v1\n{}", approval::canonical_digest(&m)).into_bytes()
}
fn verify(m: &Manifest, keys: &Value) -> Result<()> {
    let Some(d) = &m.distribution else {
        return Ok(());
    };
    if !identifier(&d.publisher)
        || d.publisher.len() > 16
        || !identifier(&d.key_id)
        || !m.id.starts_with(&format!("{}_", d.publisher))
        || !["stable", "beta", "development"].contains(&d.channel.as_str())
        || d.dependencies.len() > 12
    {
        return Err(bad(
            "Invalid signed publisher namespace, channel or dependency limit",
        ));
    }
    let public = keys[&d.publisher][&d.key_id].as_str().ok_or(Error(
        StatusCode::FORBIDDEN,
        "Publisher key is not registered by the operator".into(),
    ))?;
    let key = STANDARD
        .decode(public)
        .map_err(|_| bad("Invalid registered publisher public key"))?;
    let signature = STANDARD
        .decode(&d.signature)
        .map_err(|_| bad("Invalid package signature encoding"))?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(&message(m), &signature)
        .map_err(|_| {
            Error(
                StatusCode::FORBIDDEN,
                "Package signature does not match its content".into(),
            )
        })?;
    let mut seen = std::collections::HashSet::new();
    for dep in &d.dependencies {
        if dep.app == m.id
            || !identifier(&dep.app)
            || !identifier(&dep.publisher)
            || !dep.app.starts_with(&format!("{}_", dep.publisher))
            || !seen.insert(&dep.app)
            || semver::VersionReq::parse(&dep.version).is_err()
        {
            return Err(bad("Invalid signed app dependency"));
        }
    }
    Ok(())
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    verify(m, &crate::runtime_config::get().publishers)?;
    // A local unsigned app cannot occupy a registered publisher's namespace.
    if m.distribution.is_none()
        && crate::runtime_config::get()
            .publishers
            .as_object()
            .unwrap()
            .keys()
            .any(|publisher| m.id.starts_with(&format!("{publisher}_")))
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Registered publisher namespaces require signed packages".into(),
        ));
    }
    Ok(())
}
pub(super) async fn dependencies(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
) -> Result<()> {
    let Some(d) = &m.distribution else {
        return Ok(());
    };
    for dep in &d.dependencies {
        let row=sqlx::query("SELECT manifest,version FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE").bind(t).bind(&dep.app).fetch_optional(&mut **tx).await?.ok_or(conflict("Install and activate required app dependencies first"))?;
        let prior: Manifest = serde_json::from_value(row.get("manifest"))
            .map_err(|_| bad("Invalid dependency package"))?;
        verify(&prior, &crate::runtime_config::get().publishers)?;
        if prior
            .distribution
            .as_ref()
            .is_none_or(|v| v.publisher != dep.publisher)
            || !semver::VersionReq::parse(&dep.version).unwrap().matches(
                &semver::Version::parse(&row.get::<String, _>("version"))
                    .map_err(|_| bad("Invalid dependency version"))?,
            )
        {
            return Err(conflict(
                "Required dependency publisher or version does not match",
            ));
        }
    }
    // Upgrading an existing dependency cannot silently break active installed dependants.
    let rows =
        sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND id<>$2 AND active")
            .bind(t)
            .bind(&m.id)
            .fetch_all(&mut **tx)
            .await?;
    let graph: HashMap<String, Vec<String>> = rows
        .iter()
        .map(|row| {
            let value: Value = row.get("manifest");
            (
                value["id"].as_str().unwrap_or("").to_owned(),
                value["distribution"]["dependencies"]
                    .as_array()
                    .map(|deps| {
                        deps.iter()
                            .filter_map(|d| d["app"].as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect();
    let mut pending: Vec<String> = d.dependencies.iter().map(|d| d.app.clone()).collect();
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = pending.pop() {
        if id == m.id {
            return Err(conflict("App dependency cycle is not allowed"));
        }
        if seen.insert(id.clone()) {
            if seen.len() > 256 {
                return Err(bad("Dependency graph exceeds 256 reachable apps"));
            }
            if let Some(next) = graph.get(&id) {
                pending.extend(next.clone());
            }
        }
    }
    for row in rows {
        let value: Value = row.get("manifest");
        if let Some(deps) = value["distribution"]["dependencies"].as_array() {
            for dep in deps.iter().filter(|v| v["app"] == m.id) {
                let range = dep["version"]
                    .as_str()
                    .and_then(|s| semver::VersionReq::parse(s).ok())
                    .ok_or(bad("Invalid installed dependency constraint"))?;
                if dep["publisher"] != d.publisher
                    || !range.matches(
                        &semver::Version::parse(&m.version)
                            .map_err(|_| bad("Invalid package version"))?,
                    )
                {
                    return Err(conflict("Upgrade would break an active app dependency"));
                }
            }
        }
    }
    Ok(())
}
pub(super) async fn deactivate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    id: &str,
) -> Result<()> {
    let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_packages p WHERE tenant=$1 AND active AND id<>$2 AND EXISTS(SELECT 1 FROM jsonb_array_elements(coalesce(p.manifest->'distribution'->'dependencies','[]'::jsonb)) d WHERE d->>'app'=$2))").bind(t).bind(id).fetch_one(&mut **tx).await?;
    if used {
        return Err(conflict("App is required by another active app"));
    }
    Ok(())
}

/// Offline signing shares the server canonical representation. The 32-byte seed is supplied on stdin, never logged.
pub(crate) fn sign_package() {
    use ring::signature::KeyPair;
    use std::io::Read;
    let path = env::args()
        .nth(2)
        .expect("Usage: vendune --sign-app MANIFEST.json < BASE64_SEED_FILE");
    let mut m: Manifest = serde_json::from_slice(&std::fs::read(path).expect("Read manifest"))
        .expect("Typed manifest");
    assert!(
        m.distribution.is_some(),
        "Add publisher/keyId/channel/distribution metadata first"
    );
    let mut secret = String::new();
    std::io::stdin()
        .take(100)
        .read_to_string(&mut secret)
        .expect("Read seed from stdin");
    let mut bytes = STANDARD
        .decode(secret.trim())
        .expect("Base64 32-byte Ed25519 seed");
    secret.clear();
    let pair =
        ring::signature::Ed25519KeyPair::from_seed_unchecked(&bytes).expect("32-byte Ed25519 seed");
    bytes.fill(0);
    m.distribution.as_mut().unwrap().signature = STANDARD.encode(pair.sign(&message(&m)).as_ref());
    // Public material is useful to register the publisher; private material is never emitted.
    println!(
        "{}",
        json!({"manifest":m,"publicKey":STANDARD.encode(pair.public_key().as_ref())})
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;
    #[test]
    fn publisher_signature_covers_permissions_and_channel() {
        let seed = [7u8; 32];
        let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&seed).unwrap();
        let mut m: Manifest = serde_json::from_str(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        ))
        .unwrap();
        m.id = "fixture_engraving".into();
        m.distribution = Some(Distribution {
            publisher: "fixture".into(),
            key_id: "first".into(),
            signature: String::new(),
            channel: "stable".into(),
            dependencies: vec![],
        });
        m.distribution.as_mut().unwrap().signature =
            STANDARD.encode(key.sign(&message(&m)).as_ref());
        let keys = json!({"fixture":{"first":STANDARD.encode(key.public_key().as_ref())}});
        assert!(verify(&m, &keys).is_ok());
        m.permissions.push("customers.pii".into());
        assert!(verify(&m, &keys).is_err());
        m.permissions.pop();
        m.distribution.as_mut().unwrap().channel = "beta".into();
        assert!(verify(&m, &keys).is_err());
        assert!(verify(&m, &json!({})).is_err());
    }
}
