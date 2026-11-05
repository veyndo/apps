//! Scoped storage ports for trusted hosts; no identity resolution, pool creation, service dispatch or schema installation.
mod error;
pub mod jobs;
pub mod mutations;
pub mod progress;
pub mod queries;
pub mod records;
pub use error::Error;

/// Already admitted host scope. This key is not an authentication credential.
#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    pub tenant: &'a str,
    pub app: &'a str,
}
/// A record key always retains tenant, application, entity and identifier.
#[derive(Clone, Copy, Debug)]
pub struct RecordKey<'a> {
    pub scope: Scope<'a>,
    pub entity: &'a str,
    pub id: &'a str,
}
