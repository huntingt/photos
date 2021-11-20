use crate::{
    common::{auth_album, join, new_id, require_key, respond_ok, AppState, File},
    error::{ApiError, ApiResult},
};
use async_stream::try_stream;
use bytes::{Bytes, BytesMut};
use futures::stream::Stream;
use futures::{join, TryStreamExt};
use hyper::{header, Body, Request, Response, StatusCode};
use libvips::{ops, VipsImage};
use routerify::ext::RequestExt;
use routerify::Router;
use sled::Transactional;
use std::borrow::Cow;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tokio::{
    fs,
    io::{self, AsyncReadExt, AsyncWriteExt},
    task::block_in_place,
};
use wire::{Album, FileList, FileMetadata, ListRequest, NewResource};

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
    let metadata: FileMetadata = serde_json::from_slice(&metadata_bytes)?;

    let AppState {
        ref users,
        ref sessions,
        ref files,
        ref file_names,
        ref upload_path,
        ref medium_path,
        ref small_path,
        ref temp_path,
        ..
    } = parts.data().unwrap();

    // Don't start uploading until we have verified that the user may be able
    // to save the file
    sessions
        .get(key.as_bytes())?
        .ok_or(ApiError::Unauthorized)?;

    let file_id = new_id(16);
    let owner_file_name = [&owner_id, ".", &metadata.name].concat();

    let upload_path = upload_path.join(&file_id);
    let medium_path = medium_path.join(&file_id);
    let small_path = small_path.join(&file_id);

    let temp_id = [&file_id, ".png"].concat();
    let temp_path = temp_path.join(&temp_id);

    let mut buffer = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&upload_path)
        .await?;

    while let Some(chunk) = body.try_next().await? {
        buffer.write_all(&chunk).await.unwrap();
    }

    let result = block_in_place(|| {
        let original = if metadata.mime.starts_with("video/") {
            std::process::Command::new("ffmpeg")
                .arg("-i")
                .arg(upload_path.as_os_str())
                .arg("-vframes")
                .arg("1")
                .arg(&temp_path.to_str().unwrap())
                .output()?;
            VipsImage::new_from_file(&temp_path.to_str().unwrap())?
        } else {
            VipsImage::new_from_file(&upload_path.to_str().unwrap())?
        };

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

        respond_ok(NewResource {
            id: Cow::from(file_id),
        })
    });

    if result.is_err() {
        let _ = join!(
            fs::remove_file(&upload_path),
            fs::remove_file(&medium_path),
            fs::remove_file(&small_path),
            fs::remove_file(&temp_path)
        );
    } else {
        let _ = fs::remove_file(&temp_path).await;
    }

    result
}

async fn list(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (owner_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: ListRequest = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref file_names,
            ..
        } = parts.data().unwrap();

        sessions.get(key)?.ok_or(ApiError::Unauthorized)?;

        let prefix = [owner_id, ".", &json.prefix.unwrap_or(Cow::from(""))].concat();
        let kv_pairs = file_names
            .scan_prefix(prefix.as_bytes())
            .skip(json.skip.unwrap_or(0))
            .take(json.length.unwrap_or(usize::MAX))
            .collect::<sled::Result<Vec<(sled::IVec, sled::IVec)>>>()?;

        let file_pairs = kv_pairs
            .iter()
            .map(|(key, file_id)| {
                let (_, file_name) = std::str::from_utf8(&key).unwrap().split_once('.').unwrap();
                let file_id = std::str::from_utf8(&file_id).unwrap();
                (Cow::from(file_name), Cow::from(file_id))
            })
            .collect();

        respond_ok(FileList { files: file_pairs })
    })
}

fn file_stream(mut file: fs::File, chunk_size: usize) -> impl Stream<Item = io::Result<Bytes>> {
    try_stream! {
        loop {
            let mut buffer = BytesMut::with_capacity(chunk_size);
            file.read_buf(&mut buffer).await?;

            if buffer.is_empty() {
                break;
            }

            yield buffer.into();
        }
    }
}

async fn serve(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let quality = parts.param("quality").unwrap();
    let file_id = parts.param("fileId").unwrap();

    let AppState {
        ref sessions,
        ref files,
        ref albums,
        ref upload_path,
        ref medium_path,
        ref small_path,
        ..
    } = parts.data().unwrap();

    sessions
        .get(key.as_bytes())?
        .ok_or(ApiError::Unauthorized)?;

    let file_bytes = files.get(file_id.as_bytes())?.ok_or(ApiError::NotFound)?;
    let file: File = bincode::deserialize(&file_bytes).unwrap();

    match auth_album(&parts) {
        None => {
            if file.owner_id != user_id {
                return Err(ApiError::NotFound);
            }
        }
        Some(album_id) => {
            let album_bytes = albums
                .get(album_id.as_bytes())?
                .ok_or(ApiError::Unauthorized)?;
            let album: Album = bincode::deserialize(&album_bytes).unwrap();

            if album.owner_id != user_id {
                return Err(ApiError::Unauthorized);
            }
        }
    }

    let (path, mime): (_, &str) = match quality.as_str() {
        "large" => (upload_path.join(file_id), &file.metadata.mime),
        "medium" => (medium_path.join(file_id), "image/webp"),
        "small" => (small_path.join(file_id), "image/webp"),
        _ => return Err(ApiError::BadRequest),
    };

    let stream = file_stream(fs::File::open(path).await?, 1024 * 8);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .status(StatusCode::OK)
        .body(Body::wrap_stream(stream))
        .unwrap())
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/upload", upload)
        .get("/list", list)
        .get("/serve/:quality/:fileId", serve)
        .build()
        .unwrap()
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
