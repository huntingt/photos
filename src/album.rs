use crate::{
    common::{
        join, new_id, require_key, respond_ok, respond_ok_empty, Album, AppState, File, User,
    },
    engine::Engine,
    error::{ApiError, ApiResult},
    wire::{AlbumDescription, FileIdList, NewAlbumDetails},
};
use chrono::offset::Utc;
use hyper::{header, Body, Request, Response, StatusCode};
use routerify::{ext::RequestExt, Router};
use sled::Transactional;
use tokio::task::block_in_place;

const ALBUM_ID_BYTES: usize = 16;

async fn create(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: AlbumDescription = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref users,
            ref albums,
            ref fragments,
            ..
        } = parts.data().unwrap();

        let album_id = new_id(ALBUM_ID_BYTES);
        let album = Album {
            owner_id: &user_id,
            description: json,
            fragment_head: 0,
            length: 0,
            last_update: Utc::now().timestamp(),
            date_range: None,
        };

        (sessions, users, albums, fragments).transaction(
            |(sessions, users, albums, fragments)| {
                sessions
                    .get(key.as_bytes())?
                    .ok_or(ApiError::Unauthorized)?;
                users
                    .get(user_id.as_bytes())?
                    .ok_or(ApiError::Unauthorized)?;

                albums.insert(album_id.as_bytes(), bincode::serialize(&album).unwrap())?;
                Engine::empty(&album_id, fragments)?;

                Ok(())
            },
        )?;

        respond_ok(NewAlbumDetails {
            album_id: &album_id,
        })
    })
}

async fn add_remove(req: Request<Body>, add: bool) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: FileIdList = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref albums,
            ref files,
            ref inclusions,
            ref fragments,
            ..
        } = parts.data().unwrap();

        let album_id = parts.param("albumId").unwrap();

        (sessions, albums, inclusions, fragments, files).transaction(
            |(sessions, albums, inclusions, fragments, files)| {
                sessions
                    .get(key.as_bytes())?
                    .ok_or(ApiError::Unauthorized)?;

                let album_bytes = albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;
                let mut album: Album = bincode::deserialize(&album_bytes).unwrap();

                if album.owner_id != user_id {
                    return Err(ApiError::Unauthorized.into());
                }

                let mut e = Engine::new(&album_id, &mut album, fragments)?;
                for file_id in &json.file_ids {
                    if add {
                        let file_bytes = files.get(&file_id)?.ok_or(ApiError::Unauthorized)?;
                        let file: File = bincode::deserialize(&file_bytes).unwrap();

                        if file.owner_id != user_id {
                            return Err(ApiError::Unauthorized.into());
                        }

                        let inclusion = [file_id, ":", album_id].concat();
                        inclusions.insert(inclusion.as_bytes(), b"")?;

                        e.add(file_id, &file)?;
                    } else if let Some(file_bytes) = files.get(&file_id)? {
                        let file: File = bincode::deserialize(&file_bytes).unwrap();

                        let inclusion = [file_id, ":", album_id].concat();
                        inclusions.remove(inclusion.as_bytes())?;

                        e.remove(file_id, &file)?;
                    }
                }
                e.commit()?;

                let album_bytes = bincode::serialize(&mut album).unwrap();
                albums.insert(album_id.as_bytes(), album_bytes)?;

                Ok(())
            },
        )?;

        respond_ok_empty()
    })
}

async fn serve(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let album_id = parts.param("albumId").unwrap();
    let fragment_id = match parts.param("fragmentId").unwrap().as_str() {
        "metadata" => None,
        string => Some(string.parse().map_err(|_| ApiError::BadRequest)?),
    };

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref albums,
            ref fragments,
            ref users,
            ..
        } = parts.data().unwrap();

        sessions
            .get(key.as_bytes())?
            .ok_or(ApiError::Unauthorized)?;

        let album_bytes = albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;
        let mut album: Album = bincode::deserialize(&album_bytes).unwrap();

        if album.owner_id != user_id {
            return Err(ApiError::Unauthorized.into());
        }

        if let Some(fragment_id) = fragment_id {
            let id = Engine::get_id(&album_id, fragment_id);
            let fragment = fragments.get(id)?.ok_or(ApiError::NotFound)?;

            Ok(Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .status(StatusCode::OK)
                .body(Body::from(Vec::from(fragment.as_ref())))
                .unwrap())
        } else {
            let user_bytes = users.get(user_id)?.ok_or(ApiError::NotFound)?;
            let user: User = bincode::deserialize(&user_bytes).unwrap();
            album.owner_id = user.email;

            respond_ok(album)
        }
    })
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/create", create)
        .post("/add/:albumId", |req| add_remove(req, true))
        .delete("/remove/:albumId", |req| add_remove(req, false))
        .get("/serve/:albumId/:fragmentId", serve)
        .build()
        .unwrap()
}
