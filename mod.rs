//! Versioned app packages: managed data, UI slots, agent tools and isolated service calls.
use crate::*;
mod approval;
mod record_import;
pub(crate) use record_import::{import_record, inspect_import_record};
mod asset_surfaces;
mod business_capability;
mod cart_contributions;
mod commerce_hooks;
pub(crate) use commerce_hooks::{apply as apply_hooks, prepare as prepare_hooks};
mod compatibility;
mod configuration_events;
mod configuration_orders;
pub(crate) use configuration_orders::configurations;
mod configuration_examples;
pub(crate) use configuration_events::{configuration_budget, configuration_events};
mod configuration_inputs;
mod configuration_selection;
mod configuration_summary;
pub(crate) use configuration_selection::{
    Selection as ConfigurationSelection, resolve_selection as configuration_selection_for_list,
    resolve_selections, selection_shape,
};
mod consent;
mod core_callbacks;
pub(crate) mod core_extensions;
pub(crate) mod extension_filter;
mod extension_projection;
mod package_fence;
pub(crate) use package_fence::lock_package_write;
mod core_documents;
mod core_exchange;
mod credentials;
mod distribution;
pub(crate) use consent::actor_permissions;
pub(crate) use distribution::sign_package;
pub(crate) mod data;
mod event_contract;
mod event_projection;
mod events;
mod schedules;
mod secrets;
mod settings;
mod webhook_scope;
mod webhooks;
pub(crate) use schedules::schedule_once;
pub(crate) use webhook_scope::bind as bind_webhook_scope;
mod editor_contract;
pub(crate) use editor_contract::entity_access as record_access;
#[cfg(test)]
mod editor_tests;
mod evidence;
mod evidence_routes;
pub(crate) use evidence::*;
mod egress;
// Shared bounded public HTTPS transport; protocols must never forward customer credentials.
pub(crate) use egress::public_client as public_https_client;
mod field_values;
mod gateway;
mod tool_contract;
pub(crate) use tool_contract::app_tools;
mod input_schema;
mod job_callbacks;
mod job_evidence;
mod job_import;
mod jobs;
pub(crate) mod native_lifecycle;
pub(crate) mod record_history;
mod record_lifecycle;
pub(crate) use approval::print_digest;
pub(crate) mod hosted;
pub(crate) mod service_limits;
mod service_policy;
mod surface_grants;
mod surface_registry;
#[cfg(test)]
mod surface_tests;
pub(crate) mod surfaces;
mod ui_bundles;
pub(crate) use service_limits::ServiceLimits;
mod observability;
mod ontology;
mod planning;
mod runtime;
pub(crate) use planning::*;
mod form_layout;
mod manifest;
mod manifest_validation;
mod native_data;
#[cfg(test)]
mod native_view_tests;
mod native_views;
mod presentation;
mod publication;
#[path = "../../extensions/apps/site-builder/server.rs"]
pub(crate) mod site_builder;
#[path = "../../extensions/apps/site-forms/server.rs"]
mod site_forms;
mod submissions;
mod ui_logic;
#[cfg(test)]
mod ui_logic_tests;
mod ui_logic_types;
pub(crate) use manifest_validation::validate;
pub(crate) mod records;
mod registry;
mod relations;
mod routes;
mod schema_changes;
mod schema_upgrade;
mod storage;
pub(crate) use cart_contributions::*;
pub(crate) use commerce_hooks::verify_package;
pub(crate) use compatibility::upgrade_cart;
pub(crate) use events::*;
pub(crate) use gateway::{invoke_app, invoke_mcp, validate_input, validate_native_input};
pub(crate) use manifest::*;
pub(crate) use registry::*;
pub(crate) use routes::*;

pub(crate) use service_policy::allowed as service_origin_allowed;
