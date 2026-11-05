//! Pure app data contracts and validation; no database, network, identity resolution or commerce mutation authority.
pub mod admission;
pub mod declarations;
mod error;
pub mod events;
pub mod evolution;
pub mod fields;
pub mod intelligence;
pub mod language;
pub mod lifecycle;
pub mod object_extensions;
pub mod ontology;
pub mod package;
mod package_actions;
pub mod publication;
pub mod schema;
mod schema_contract;
pub mod settings;
pub mod subscriptions;
pub mod triggers;
mod types;
pub use error::{Error, Kind};
pub use types::{Action, Choice, Entity, Extension, Field, Object, Slot};
pub type Result<T> = std::result::Result<T, Error>;
fn bad(message: impl Into<String>) -> Error {
    Error {
        kind: Kind::Bad,
        message: message.into(),
        pointer: None,
    }
}
fn conflict(message: impl Into<String>) -> Error {
    Error {
        kind: Kind::Conflict,
        message: message.into(),
        pointer: None,
    }
}
