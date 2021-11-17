use crate::error::{ApiError, ApiResult};
use crate::wire::Metadata;
use hyper::Body;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct User<'a> {
    pub email: &'a str,
    pub password: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct File<'a> {
    pub owner_id: &'a str,
    pub width: i32,
    pub height: i32,
    pub metadata: Metadata<'a>,
}

pub struct AppState {
    pub users: sled::Tree,
    pub emails: sled::Tree,
    pub sessions: sled::Tree,
    pub files: sled::Tree,

    pub argon_config: argon2::Config<'static>,
    pub upload_path: PathBuf,
    pub medium_path: PathBuf,
    pub small_path: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        let db = sled::Config::new().temporary(true).open().unwrap();

        AppState {
            users: db.open_tree(b"users").unwrap(),
            emails: db.open_tree(b"emails").unwrap(),
            sessions: db.open_tree(b"sessions").unwrap(),
            files: db.open_tree(b"files").unwrap(),

            argon_config: argon2::Config::default(),

            upload_path: PathBuf::from("data/uploads"),
            medium_path: PathBuf::from("data/medium"),
            small_path: PathBuf::from("data/small"),
        }
    }

    pub fn create_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.upload_path)?;
        std::fs::create_dir_all(&self.medium_path)?;
        std::fs::create_dir_all(&self.small_path)?;
        Ok(())
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
    let query_str = parts.uri.query().ok_or(ApiError::Unauthorized)?;
    let queries = querystring::querify(query_str);
    let (_, key) = queries
        .iter()
        .find(|(k, _)| k == &"key")
        .ok_or(ApiError::Unauthorized)?;
    Ok(key)
}

pub fn new_id(size: usize) -> String {
    let bytes: Vec<u8> = (0..size).map(|_| thread_rng().gen()).collect();
    base64::encode_config(&bytes, base64::URL_SAFE_NO_PAD)
}
