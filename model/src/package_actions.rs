//! Original bounded action grammar and cross-feature restrictions shared by every package compiler consumer.
use crate::package::{Contract, Policy};
use crate::{Result, bad, declarations::identifier, schema as input_schema};
pub(super) fn validate<P: Policy>(m: &Contract<'_>, policy: &P) -> Result<()> {
    let mut actions = std::collections::HashSet::new();
    for a in m.actions {
        input_schema::validate_contract(&a.input_schema)?;
        if let Some(schema) = &a.output_schema {
            input_schema::validate_contract(schema)?;
        }
        if !identifier(&a.name)
            || !actions.insert(&a.name)
            || a.description.len() > 300
            || ![
                "list",
                "save",
                "service",
                "configurations",
                "configure",
                "emit",
                "submit",
                "payment_onboarding",
                "payment_command",
                "job",
                "assets",
                "asset_preview",
                "asset_upload",
            ]
            .contains(&a.handler.as_str())
            || a.permission
                .as_deref()
                .is_some_and(|p| !policy.known_scope(p))
            || a.required_permissions.len() > 8
            || a.required_permissions
                .iter()
                .any(|p| !policy.known_scope(p))
            || (a.public && !a.required_permissions.is_empty())
            || (a.handler == "emit" && !m.permissions.contains(&"events.publish".into()))
            || (a.public && a.permission.is_some())
            || (a.handler == "job"
                && (a.public
                    || a.read_only
                    || m.runtime != "service"
                    || !m.permissions.iter().any(|p| p == "jobs.write")
                    || !m.events.iter().any(|e| {
                        e == &format!("app.{}.job_{}", m.id, a.name)
                            || e == &format!("app.{}.job_queued", m.id)
                    })))
            || (a.handler == "emit" && a.public)
            || (a.handler == "emit" && a.read_only)
            || (a.handler == "payment_command"
                && (a.public
                    || a.read_only
                    || a.permission.as_deref() != Some("payments.manage")
                    || !m.has_payment_provider))
            || (a.handler == "payment_onboarding"
                && (a.public
                    || a.read_only
                    || a.permission.as_deref() != Some("payments.manage")
                    || !m.has_payment_provider
                    || a.flow_allowed))
            || a.input_schema["type"] != "object"
            || (["assets", "asset_preview", "asset_upload"].contains(&a.handler.as_str())
                && (a.public
                    || (a.handler != "asset_upload" && !a.read_only)
                    || !m.permissions.iter().any(|p| p == "assets.read")
                    || a.handler == "asset_upload"
                        && (!m.permissions.iter().any(|p| p == "assets.write") || a.read_only)))
            || a.input_schema["additionalProperties"] != false
            || (a.handler == "save" && a.public)
            || (a.handler == "configurations" && a.public)
            || (a.handler == "configure"
                && (m.configuration.is_none() || !a.public || a.read_only || a.flow_allowed))
            || a.entity
                .as_ref()
                .is_some_and(|n| !m.entities.iter().any(|e| e.name == *n))
        {
            return Err(bad("Invalid app action"));
        }
        if a.handler == "submit"
            && (!a.public
                || a.read_only
                || a.flow_allowed
                || a.entity
                    .as_ref()
                    .is_none_or(|n| !m.entities.iter().any(|e| e.name == *n && !e.public_read))
                || !m.permissions.iter().any(|p| p == "data.write"))
        {
            return Err(bad("Submit requires an append-only private entity"));
        }
        if ["list", "save", "submit"].contains(&a.handler.as_str()) && a.entity.is_none() {
            return Err(bad("Entity action requires entity"));
        }
        if a.public
            && a.handler != "submit"
            && a.entity
                .as_ref()
                .is_some_and(|n| !m.entities.iter().any(|e| e.name == *n && e.public_read))
        {
            return Err(bad("Private entity cannot be a public tool"));
        }
    }
    Ok(())
}
