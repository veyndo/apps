//! Versioned app packages: managed data, UI slots, agent tools and isolated service calls.
use crate::*;
mod cart_contributions;
mod compatibility;
pub(crate) mod data;
mod events;
mod schedules;
mod webhooks;
pub(crate) use schedules::schedule_once;
mod editor_contract;
#[cfg(test)]
mod editor_tests;
mod evidence;
mod evidence_routes;
pub(crate) use evidence::*;
mod gateway;
pub(crate) mod hosted;
mod service_limits;
mod service_policy;
#[cfg(test)]
mod surface_tests;
mod surfaces;
pub(crate) use service_limits::ServiceLimits;
mod planning;
mod runtime;
pub(crate) use planning::*;
mod manifest;
mod manifest_validation;
mod native_data;
#[cfg(test)]
mod native_view_tests;
mod native_views;
mod presentation;
pub(crate) use manifest_validation::validate;
mod registry;
mod routes;
pub(crate) use cart_contributions::*;
pub(crate) use compatibility::upgrade_cart;
pub(crate) use events::*;
pub(crate) use gateway::{app_tools, invoke_app, invoke_mcp, validate_input};
pub(crate) use manifest::*;
pub(crate) use registry::*;
pub(crate) use routes::*;
