//! Signed namespace/dependency grammar and pure installed-graph checks; cryptographic trust is a mandatory host port.
use crate::{Result, bad, conflict, declarations::identifier, manifest::Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Distribution {
    pub publisher: String,
    pub key_id: String,
    pub signature: String,
    pub channel: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dependency {
    pub app: String,
    pub publisher: String,
    pub version: String,
}

pub trait SignatureVerifier {
    type Error;
    /// Verify against current operator-registered public keys and the complete canonical package message.
    fn verify_signature(&self, manifest: &Manifest) -> std::result::Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum VerifyError<E> {
    Model(crate::Error),
    Host(E),
}
pub fn verify<V: SignatureVerifier>(
    m: &Manifest,
    verifier: &V,
) -> std::result::Result<(), VerifyError<V::Error>> {
    let Some(d) = &m.distribution else {
        return Ok(());
    };
    envelope(m, d).map_err(VerifyError::Model)?;
    verifier.verify_signature(m).map_err(VerifyError::Host)?;
    dependencies(m, d).map_err(VerifyError::Model)
}
fn envelope(m: &Manifest, d: &Distribution) -> Result<()> {
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
    Ok(())
}
fn dependencies(m: &Manifest, d: &Distribution) -> Result<()> {
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
/// Compare an already signature-verified scoped installed package to its declared dependency.
pub fn require_version(dep: &Dependency, prior: &Manifest, installed_version: &str) -> Result<()> {
    if prior
        .distribution
        .as_ref()
        .is_none_or(|v| v.publisher != dep.publisher)
        || !semver::VersionReq::parse(&dep.version)
            .map_err(|_| bad("Invalid signed app dependency"))?
            .matches(
                &semver::Version::parse(installed_version)
                    .map_err(|_| bad("Invalid dependency version"))?,
            )
    {
        return Err(conflict(
            "Required dependency publisher or version does not match",
        ));
    }
    Ok(())
}
/// Input rows must come from the host's current tenant-scoped, locked active-package query.
pub fn validate_graph(m: &Manifest, installed: &[Value]) -> Result<()> {
    let Some(d) = &m.distribution else {
        return Ok(());
    };
    let graph: HashMap<String, Vec<String>> = installed
        .iter()
        .map(|row| {
            let value = row;
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
    for row in installed {
        let value = row;
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
