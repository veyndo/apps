//! One current nondelegatable administrator admission for App host management; introspection, not request fields, supplies these facts.
use crate::{
    error::{Problem, Result},
    identity::Principal,
};
use axum::http::StatusCode;
pub fn admit(p: &Principal) -> Result<()> {
    if !p.merchant
        || !p.personal_session
        || !p.live
        || p.subject == "bootstrap"
        || !p.permits("apps.manage")
        || !p.permits("team.manage")
    {
        return Err(Problem::new(
            StatusCode::FORBIDDEN,
            "management_authority",
            "A current personal live-shop administrator session with apps.manage and team.manage is required",
        ));
    }
    Ok(())
}
