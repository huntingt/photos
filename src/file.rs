use crate::{
    common::{join, new_id, require_key, AppState, File},
    error::{ApiError, ApiResult},
    wire::{FileList, ListParams, Metadata},
};
use futures::{join, TryStreamExt};
use hyper::{Body, Request, Response, StatusCode};
use libvips::{ops, VipsImage};
use routerify::ext::RequestExt;
use routerify::Router;
use sled::Transactional;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tokio::{fs, io::AsyncWriteExt, task::block_in_place};

const UPLOAD_METADATA: &'static str = "upload-metadata";
const MEDIUM_HEIGHT: f64 = 400.;
const SMALL_HEIGHT: f64 = 10.;

async fn upload(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, mut body) = req.into_parts();

    let key = require_key(&parts)?;
    let (owner_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let metadata_header = parts
        .headers
        .get(UPLOAD_METADATA)
        .ok_or(ApiError::BadRequest)?;
    let metadata_bytes = base64::decode_config(metadata_header, base64::URL_SAFE)
        .map_err(|_| ApiError::BadRequest)?;
    let metadata: Metadata = serde_json::from_slice(&metadata_bytes)?;

    let AppState {
        ref users,
        ref sessions,
        ref files,
        ref file_names,
        ref upload_path,
        ref medium_path,
        ref small_path,
        ..
    } = parts.data().unwrap();

    // Don't start uploading until we have verified that the user may be able
    // to save the file
    sessions
        .get(key.as_bytes())?
        .ok_or(ApiError::Unauthorized)?;

    let file_id = new_id(16);
    let owner_file_name = [&owner_id, ".", metadata.name].concat();

    let upload_path = upload_path.join(&file_id);
    let medium_path = medium_path.join(&file_id);
    let small_path = small_path.join(&file_id);

    let mut buffer = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&upload_path)
        .await?;

    while let Some(chunk) = body.try_next().await? {
        buffer.write_all(&chunk).await.unwrap();
    }

    let result = block_in_place(|| {
        let original = VipsImage::new_from_file(&upload_path.to_str().unwrap())?;
        let rotated = ops::autorot(&original).unwrap();

        let height = rotated.get_height();
        let width = rotated.get_width();

        let medium_factor = MEDIUM_HEIGHT / height as f64;
        let medium = ops::resize(&rotated, medium_factor)?;
        ops::webpsave(&medium, medium_path.to_str().unwrap())?;

        let small_factor = SMALL_HEIGHT / MEDIUM_HEIGHT;
        let small = ops::resize(&medium, small_factor)?;
        ops::webpsave(&small, small_path.to_str().unwrap())?;

        let file = File {
            owner_id,
            width,
            height,
            metadata,
        };

        (users, files, file_names).transaction(|(users, files, file_names)| {
            users.get(owner_id)?.ok_or(ApiError::Unauthorized)?;
            files.insert(file_id.as_bytes(), bincode::serialize(&file).unwrap())?;

            match file_names.insert(owner_file_name.as_bytes(), file_id.as_bytes())? {
                Some(_) => Err(ApiError::FileExists.into()),
                None => Ok(()),
            }
        })?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(file_id))
            .unwrap())
    });

    if result.is_err() {
        let _ = join!(
            fs::remove_file(&upload_path),
            fs::remove_file(&medium_path),
            fs::remove_file(&small_path)
        );
    }

    result
}

async fn list(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (owner_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: ListParams = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref file_names,
            ..
        } = parts.data().unwrap();

        sessions.get(key)?.ok_or(ApiError::Unauthorized)?;

        let mut file_pairs = vec![];

        let start = match json.start {
            Some(start) => [owner_id, ".", start].concat(),
            None => [&owner_id, "."].concat(),
        };
        let end = [owner_id.as_bytes(), &[255u8]].concat();

        for maybe_pair in file_names.range(start.as_bytes()..&end) {
            if let Some(length) = json.length {
                if file_pairs.len() >= length {
                    break;
                }
            }

            let (key, file_id_bytes) = maybe_pair?;

            let key_str = std::str::from_utf8(&key).unwrap();
            let (_, file_name) = key_str.split_once('.').unwrap();

            let file_id = std::str::from_utf8(&file_id_bytes).unwrap();

            file_pairs.push((file_name.to_owned(), file_id.to_owned()));
        }

        let response = serde_json::to_string(&FileList { files: file_pairs })?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(response))
            .unwrap())
    })
}

async fn clean_path(app_state: &AppState, path: &Path) -> ApiResult<usize> {
    let mut removed = 0;

    let mut iter = fs::read_dir(path).await?;
    while let Some(entry) = iter.next_entry().await? {
        let path = entry.path();
        if let Some(file_name) = path.file_name() {
            if app_state.files.get(file_name.as_bytes())?.is_none() {
                if fs::remove_file(path).await.is_ok() {
                    removed += 1;
                }
            }
        }
    }

    Ok(removed)
}

pub async fn clean_files(app_state: &AppState) -> ApiResult<usize> {
    let (a, b, c) = join!(
        clean_path(app_state, &app_state.upload_path),
        clean_path(app_state, &app_state.medium_path),
        clean_path(app_state, &app_state.small_path)
    );

    Ok(a? + b? + c?)
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/upload", upload)
        .get("/list", list)
        .build()
        .unwrap()
}
