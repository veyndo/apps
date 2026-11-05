//! Bounded core-Wasm and WIT component execution; no database, HTTP, identity, WASI or environment authority.
pub mod component_runtime;
pub mod policy;
pub mod sandbox;
mod sandbox_engine;
#[cfg(test)]
mod test_host;
