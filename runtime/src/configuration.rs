//! Canonical exact-integer contribution execution; actual host currency conversion and applying prices remain external.
use crate::sandbox::Sandbox;
pub fn run(
    guest: &Sandbox,
    fee: i64,
    arguments: Option<Vec<i64>>,
    arity: usize,
) -> Result<i64, String> {
    let result = if let Some(arguments) = arguments {
        guest.contribution_args(fee, &arguments)?
    } else if guest.validate_contribution_args(fee, arity)? {
        fee
    } else {
        return Err("App rejected price data".into());
    };
    if !(0..=100_000_000).contains(&result) {
        return Err("App rejected configuration or exceeded platform money bound".into());
    }
    Ok(result)
}
