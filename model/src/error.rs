//! Transport-neutral validation errors preserve the native bad-input, conflict and JSON-pointer distinctions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bad,
    Conflict,
    Validation,
}
#[derive(Debug)]
pub struct Error {
    pub kind: Kind,
    pub message: String,
    pub pointer: Option<String>,
}
impl Error {
    pub fn validation(path: &str, message: impl Into<String>) -> Self {
        Self {
            kind: Kind::Validation,
            pointer: Some(path.into()),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
