use std::fmt;
use hyper::Body;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    NotFound,
    BadRequest,
    Hyper(hyper::Error),
    Json(serde_json::Error),
    R2D2(r2d2::Error),
    Sqlite(rusqlite::Error),
    Argon(argon2::Error),
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

impl From<r2d2::Error> for ApiError {
    fn from(error: r2d2::Error) -> Self {
        ApiError::R2D2(error)
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> Self {
        ApiError::Sqlite(error)
    }
}

impl From<argon2::Error> for ApiError {
    fn from(error: argon2::Error) -> Self {
        ApiError::Argon(error)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

use r2d2_sqlite::SqliteConnectionManager;

pub struct AppState {
    pub pool: r2d2::Pool<SqliteConnectionManager>,
    pub argon_config: argon2::Config<'static>,
}

impl AppState {
    pub fn new() -> Self {
        let manager = SqliteConnectionManager::memory();
        let pool = r2d2::Pool::new(manager).unwrap();

        pool.get()
            .unwrap()
            .execute_batch(include_str!("setup.sql"))
            .unwrap();

        AppState {
            pool: pool,
            argon_config: argon2::Config::default(),
        }
    }
}

pub async fn join(body: Body) -> ApiResult<Vec<u8>> {
    use futures::TryStreamExt;

    let mut data = vec![];
    let mut stream = body.into_stream();

    while let Some(chunk) = stream.try_next().await? {
        data.extend_from_slice(&chunk);
    }
    
    Ok(data)
}

pub fn require_key(parts: &hyper::http::request::Parts) -> ApiResult<&str> {
    let query_str = parts.uri.query()
        .ok_or(ApiError::Unauthorized)?;
    let queries = querystring::querify(query_str);
    let (_, key) = queries.iter().find(|(k, _)| k == &"key")
        .ok_or(ApiError::Unauthorized)?;
    Ok(key)
}
