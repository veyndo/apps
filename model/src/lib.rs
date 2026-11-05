//! Pure app data contracts and validation; no database, network, identity resolution or commerce mutation authority.
pub mod admission;
pub mod declarations;
mod error;
pub mod fields;
pub mod language;
pub mod lifecycle;
pub mod publication;
pub mod schema;
mod schema_contract;
mod types;
pub use error::{Error, Kind};
pub use types::{Action, Choice, Entity, Extension, Field, Object};
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
