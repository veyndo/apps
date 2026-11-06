//! Frozen commerce decisions execute the real bounded guest; no model predictions or second pricing engine.
use crate::{component_runtime as wit, sandbox::Sandbox};
use veyndo_app_model::{
    Error, Result,
    business_capability::{Contract, Expected},
    business_snapshot::Amount,
};
pub fn verify(contract: Option<&Contract>, guest: &Sandbox) -> Result<()> {
    for (index, example) in contract.iter().flat_map(|c| &c.examples).enumerate() {
        let kind = match example.hook.as_str() {
            "price" => wit::Kind::Price,
            "shipping" => wit::Kind::Shipping,
            "discount" => wit::Kind::Discount,
            _ => wit::Kind::Validation,
        };
        let path = format!("/capability/examples/{index}/expected");
        let out = guest
            .evaluate(
                crate::business_snapshot::snapshot(&example.snapshot),
                kind,
                &example.context.to_string(),
            )
            .map_err(|_| {
                Error::validation(
                    &path,
                    format!(
                        "Business example {} could not execute within the component contract",
                        example.id
                    ),
                )
            })?;
        let actual = Expected {
            adjustment: Amount {
                minor: out.adjustment.minor,
                currency: out.adjustment.currency,
                scale: out.adjustment.scale,
            },
            accepted: out.accepted,
            reason_code: out.reason_code,
        };
        if actual != example.expected {
            return Err(Error::validation(
                &path,
                format!(
                    "Business example {} does not match its expected decision; review the example or component",
                    example.id
                ),
            ));
        }
    }
    Ok(())
}
