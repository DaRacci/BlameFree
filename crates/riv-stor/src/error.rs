use std::fmt;

/// Errors that can occur during storage operations.
#[derive(Debug)]
pub enum Error {
    /// Connection/pool initialization failure.
    Connection(String),

    /// SQL query or SeaORM execution failure.
    Query(String),

    /// Item with the given ID was not found.
    NotFound(String),

    /// Schema migration failure.
    Migration(String),

    /// Internal/unexpected error.
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Connection(msg) => write!(f, "connection error: {msg}"),
            Error::Query(msg) => write!(f, "query error: {msg}"),
            Error::NotFound(id) => write!(f, "item not found: {id}"),
            Error::Migration(msg) => write!(f, "migration error: {msg}"),
            Error::Internal(err) => write!(f, "internal error: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Internal(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}
