//! Bounded core-Wasm and WIT component execution; no database, HTTP, identity, WASI or environment authority.
pub mod business_examples;
pub mod business_snapshot;
pub mod component_runtime;
pub mod configuration;
pub mod policy;
pub mod sandbox;
mod sandbox_engine;
#[cfg(test)]
mod test_host;
