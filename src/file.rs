use crate::common::{require_key, ApiError, ApiResult, AppState};
use futures::TryStreamExt;
use hyper::{Body, Request, Response, StatusCode};
use libvips::{ops, VipsImage};
use rand::{thread_rng, Rng};
use routerify::ext::RequestExt;
use routerify::Router;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::task::block_in_place;

const UPLOAD_METADATA: &'static str = "upload-metadata";
const MEDIUM_HEIGHT: f64 = 400.;
const SMALL_HEIGHT: f64 = 10.;

#[derive(Deserialize)]
struct Metadata<'a> {
    last_modified: i64,
    name: &'a str,
    mime: &'a str,
}

async fn upload(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, mut body) = req.into_parts();

    let metadata_header = parts
        .headers
        .get(UPLOAD_METADATA)
        .ok_or(ApiError::BadRequest)?;
    let metadata_bytes = base64::decode_config(metadata_header, base64::URL_SAFE)
        .map_err(|_| ApiError::BadRequest)?;
    let metadata: Metadata = serde_json::from_slice(&metadata_bytes)?;

    let key = require_key(&parts)?;

    let file_id_bytes: [u8; 16] = thread_rng().gen();
    let file_id = base64::encode_config(&file_id_bytes, base64::URL_SAFE_NO_PAD);

    let app_state = parts.data::<AppState>().unwrap();
    let mut db = app_state.pool.get()?;

    let user_id: i64 = block_in_place(|| {
        db.query_row(
            "SELECT user_id FROM sessions WHERE key = ?",
            params![key],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(ApiError::Unauthorized)
    })?;

    let mut file_path = app_state.upload_path.clone();
    file_path.push(&file_id);

    let mut medium_path = app_state.medium_path.clone();
    medium_path.push(&file_id);

    let mut small_path = app_state.small_path.clone();
    small_path.push(&file_id);

    let mut buffer = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&file_path)
        .await?;

    while let Some(chunk) = body.try_next().await? {
        buffer.write_all(&chunk).await.unwrap();
    }

    let result = block_in_place(|| {
        let original = VipsImage::new_from_file(&file_path.to_str().unwrap())?;
        let rotated = ops::autorot(&original).unwrap();

        let height = rotated.get_height();
        let width = rotated.get_width();

        let medium_factor = MEDIUM_HEIGHT / height as f64;
        let medium = ops::resize(&rotated, medium_factor)?;
        ops::webpsave(&medium, medium_path.to_str().unwrap())?;

        let small_factor = SMALL_HEIGHT / MEDIUM_HEIGHT;
        let small = ops::resize(&medium, small_factor)?;
        ops::webpsave(&small, small_path.to_str().unwrap())?;

        let tx = db.transaction()?;

        // Double check that the user wasn't deleted during the upload
        /* TODO
        if None == tx.query_row(
                "SELECT user_id FROM users WHERE user_id = ?",
                params![key],
                |_row| Ok(true),
            ).optional()? {
            return Err(ApiError::Unauthorized);
        }
        */

        tx.execute(
            "INSERT INTO files(
                file_id, owner_id,
                width, height,
                last_modified, name, mime)
            VALUES(?, ?, ?, ?, ?, ?, ?)",
            params![
                file_id,
                user_id,
                width,
                height,
                metadata.last_modified,
                metadata.name,
                metadata.mime
            ],
        )?;

        tx.commit()?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(file_id))
            .unwrap())
    });

    if result.is_err() {
        tokio::fs::remove_file(&file_path).await?;
        tokio::fs::remove_file(&medium_path).await?;
        tokio::fs::remove_file(&small_path).await?;
    }

    result
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder().post("/upload", upload).build().unwrap()
}
