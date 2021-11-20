use sled::transaction::{ConflictableTransactionError, TransactionError};
use std::fmt;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    NotFound,
    BadRequest,
    EmailTaken,
    FileExists,
    Hyper(hyper::Error),
    Json(serde_json::Error),
    Sled(sled::Error),
    Argon(argon2::Error),
    IO(std::io::Error),
    Vips(libvips::error::Error),
}

impl std::error::Error for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<hyper::Error> for ApiError {
    fn from(error: hyper::Error) -> Self {
        ApiError::Hyper(error)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        ApiError::Json(error)
    }
}

impl From<sled::Error> for ApiError {
    fn from(error: sled::Error) -> Self {
        ApiError::Sled(error)
    }
}

impl From<argon2::Error> for ApiError {
    fn from(error: argon2::Error) -> Self {
        ApiError::Argon(error)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        ApiError::IO(error)
    }
}

impl From<libvips::error::Error> for ApiError {
    fn from(error: libvips::error::Error) -> Self {
        ApiError::Vips(error)
    }
}

impl From<TransactionError<ApiError>> for ApiError {
    fn from(error: TransactionError<ApiError>) -> Self {
        use TransactionError::*;
        match error {
            Abort(error) => error,
            Storage(error) => error.into(),
        }
    }
}

impl From<ApiError> for ConflictableTransactionError<ApiError> {
    fn from(error: ApiError) -> Self {
        ConflictableTransactionError::Abort(error)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
