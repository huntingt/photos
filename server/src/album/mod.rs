mod share;
pub mod engine;


use crate::{
    delete,
    common::{
        join, new_id, require_key, respond_ok, respond_ok_empty, test_logged_in, AppState, File,
    },
    error::{ApiError, ApiResult},
};
use engine::Engine;
use std::collections::HashMap;
use chrono::offset::Utc;
use hyper::{header, Body, Request, Response, StatusCode};
use routerify::{ext::RequestExt, Router};
use share::test_user_can_write;
use sled::Transactional;
use std::borrow::Cow;
use tokio::task::block_in_place;
use wire::{Album, AlbumSettings, IdList, NewResource, Role};

const ALBUM_ID_BYTES: usize = 16;

async fn create(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: AlbumSettings = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref users,
            ref albums,
            ref fragments,
            ref user_to_album,
            ref album_to_user,
            ..
        } = parts.data().unwrap();

        let album_id = new_id(ALBUM_ID_BYTES);
        let album = Album {
            description: json,
            fragment_head: 0,
            length: 0,
            last_update: Utc::now().timestamp(),
            date_range: None,
        };

        test_logged_in(sessions, key)?;

        (users, albums, fragments, user_to_album, album_to_user).transaction(
            |(users, albums, fragments, user_to_album, album_to_user)| {
                users
                    .get(user_id.as_bytes())?
                    .ok_or(ApiError::Unauthorized)?;

                albums.insert(album_id.as_bytes(), bincode::serialize(&album).unwrap())?;
                Engine::empty(&album_id, fragments)?;

                let role = Role::Owner;
                let role_bytes = bincode::serialize(&role).unwrap();

                user_to_album.insert([user_id, ".", &album_id].concat().as_bytes(), role_bytes)?;
                album_to_user.insert([&album_id, ".", user_id].concat().as_bytes(), b"")?;

                Ok(())
            },
        )?;

        respond_ok(NewResource {
            id: Cow::from(album_id),
        })
    })
}

async fn update(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: AlbumSettings = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref user_to_album,
            ref albums,
            ref fragments,
            ref files,
            ..
        } = parts.data().unwrap();

        test_logged_in(sessions, key)?;

        let album_id = parts.param("albumId").unwrap();

        (albums, fragments, files, user_to_album).transaction(
            |(albums, fragments, files, user_to_album)| {
                test_user_can_write(user_to_album, user_id, album_id)?;

                let prev_album_bytes = albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;
                let mut album: Album = bincode::deserialize(&prev_album_bytes).unwrap();

                if album.description.time_zone != json.time_zone {
                    let mut e = Engine::new(album_id, &mut album, fragments)?;

                    let file_ids = e.list_file_ids()?;
                    e.clear_all()?;

                    for file_id in file_ids {
                        if let Some(file_bytes) = files.get(&file_id)? {
                            let file: File = bincode::deserialize(&file_bytes).unwrap();
                            e.add(&file_id, &file)?;
                        }
                    }

                    e.commit()?;
                }

                album.description = json.clone();
                let album_bytes = bincode::serialize(&album).unwrap();

                albums.insert(album_id.as_bytes(), album_bytes)?.unwrap();

                Ok(())
            },
        )?;

        respond_ok_empty()
    })
}

async fn delete(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    block_in_place(|| {
        let state = parts.data().unwrap();
        let AppState {
            ref sessions,
            ref user_to_album,
            ..
        } = state;

        test_logged_in(sessions, key)?;
        
        let album_id = parts.param("albumId").unwrap();

        // The album may actually transfer here and end up being deleted
        // after the transfer. This is okay because it preserves the database
        // invariants even if it may look strange to the end user.
        let user_bytes = user_to_album
            .get([user_id, ".", album_id].concat())?
            .ok_or(ApiError::Unauthorized)?;
        let user_role: Role = bincode::deserialize(&user_bytes).unwrap();
        if !user_role.is_owner() {
            return Err(ApiError::Unauthorized);
        }

        delete::Command::Album(album_id).run(state)?;

        respond_ok_empty()
    })
}

async fn list(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref user_to_album,
            ref albums,
            ..
        } = parts.data().unwrap();

        test_logged_in(sessions, key)?;

        let mut album_pairs = HashMap::new();

        for entry in user_to_album.scan_prefix(&user_id) {
            let (key, role_bytes) = entry?;
            let (_, album_id) = std::str::from_utf8(&key)
                .unwrap()
                .split_once('.')
                .unwrap();

            let role: Role = bincode::deserialize(&role_bytes).unwrap();

            if let Some(album_bytes) = albums.get(&album_id)? {
                let album: Album = bincode::deserialize(&album_bytes).unwrap();
                let mut value = serde_json::to_value(album)?;
                if let serde_json::Value::Object(ref mut map) = value {
                    map.insert("role".to_string(), serde_json::to_value(role)?);
                } else {
                    panic!("Expected album to be a json object");
                }
                
                album_pairs.insert(album_id.to_string(), value);
            }
        }

        respond_ok(album_pairs)
    })
}

async fn add_remove(req: Request<Body>, add: bool) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: IdList = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref albums,
            ref files,
            ref inclusions,
            ref fragments,
            ref user_to_album,
            ..
        } = parts.data().unwrap();

        let album_id = parts.param("albumId").unwrap();

        test_logged_in(sessions, key)?;

        (albums, inclusions, fragments, files, user_to_album).transaction(
            |(albums, inclusions, fragments, files, user_to_album)| {
                test_user_can_write(user_to_album, user_id, album_id)?;

                let album_bytes = albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;
                let mut album: Album = bincode::deserialize(&album_bytes).unwrap();

                let mut e = Engine::new(&album_id, &mut album, fragments)?;
                for file_id in &json.ids {
                    if add {
                        let file_bytes = files.get(&**file_id)?.ok_or(ApiError::Unauthorized)?;
                        let file: File = bincode::deserialize(&file_bytes).unwrap();

                        if file.owner_id != user_id {
                            return Err(ApiError::Unauthorized.into());
                        }

                        let inclusion = [file_id, ".", album_id].concat();
                        inclusions.insert(inclusion.as_bytes(), b"")?;

                        e.add(file_id, &file)?;
                    } else if let Some(file_bytes) = files.get(&**file_id)? {
                        let file: File = bincode::deserialize(&file_bytes).unwrap();

                        let inclusion = [file_id, ".", album_id].concat();
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
            ref user_to_album,
            ..
        } = parts.data().unwrap();

        test_logged_in(sessions, key)?;

        let role_bytes = user_to_album
            .get([user_id, ".", album_id].concat())?
            .ok_or(ApiError::Unauthorized)?;
        let role: Role = bincode::deserialize(&role_bytes).unwrap();

        if let Some(fragment_id) = fragment_id {
            let id = Engine::get_id(&album_id, fragment_id);
            let fragment = fragments.get(id)?.ok_or(ApiError::NotFound)?;

            Ok(Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .status(StatusCode::OK)
                .body(Body::from(Vec::from(fragment.as_ref())))
                .unwrap())
        } else {
            let album_bytes = albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;
            let album: Album = bincode::deserialize(&album_bytes).unwrap();

            let mut value = serde_json::to_value(album)?;
            if let serde_json::Value::Object(ref mut map) = value {
                map.insert("role".to_string(), serde_json::to_value(role)?);
            } else {
                panic!("Expected album to be a json object");
            }

            respond_ok(value)
        }
    })
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/", create)
        .get("/", list)
        .delete("/:albumId", delete)
        .patch("/:albumId", update)
        .post("/:albumId/files", |req| add_remove(req, true))
        .delete("/:albumId/files", |req| add_remove(req, false))
        .get("/:albumId/serve/:fragmentId", serve)
        .scope("/:albumId/share", share::router())
        .build()
        .unwrap()
}
