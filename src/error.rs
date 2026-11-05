//! Preserve database errors and explicit idempotency/quota conflicts without inventing HTTP or transport authority.
#[derive(Debug)]
pub enum Error {
    Database(sqlx::Error),
    Conflict(&'static str),
    Quota(&'static str),
}
impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(e) => write!(f, "{e}"),
            Self::Conflict(s) | Self::Quota(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for Error {}
