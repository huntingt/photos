use tokio::io;
use std::fmt;
use async_trait::async_trait;
use reqwest::{Url, Response};

#[derive(Debug)]
pub enum Error {
    Remote {
        status_code: reqwest::StatusCode,
        url: Url,
        details: String,
    },
    Reqwest(reqwest::Error),
    IO(io::Error),
    Json(serde_json::Error),
    Sled(sled::Error),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Error::Reqwest(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IO(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error)
    }
}

impl From<sled::Error> for Error {
    fn from(error: sled::Error) -> Self {
        Error::Sled(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[async_trait]
pub trait ResponseErrorExt: Sized {
    async fn check_status(self) -> Result<Self>;
}

#[async_trait]
impl ResponseErrorExt for Response {
    async fn check_status(self) -> Result<Self> {
        if !self.status().is_success() {
            Err(Error::Remote {
                status_code: self.status(),
                url: self.url().clone(),
                details: self.text().await?,
            })
        } else {
            Ok(self)
        }
    }
}
