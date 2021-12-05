use crate::{
    common::{
        join, require_key, respond_ok, respond_ok_empty, test_logged_in, AppState, File, User,
    },
    error::{ApiError, ApiResult},
};
use super::engine::Engine;
use hyper::{Body, Request, Response};
use routerify::{ext::RequestExt, Router};
use sled::transaction::abort;
use sled::transaction::ConflictableTransactionResult;
use sled::transaction::TransactionalTree;
use sled::Transactional;
use std::borrow::Cow;
use tokio::task::block_in_place;
use wire::{Album, Key, PermissionPair, Role};

pub fn test_user_can_write(
    user_to_album: &TransactionalTree,
    user_id: &str,
    album_id: &str,
) -> ConflictableTransactionResult<(), ApiError> {
    let user_bytes = user_to_album
        .get([user_id, ".", album_id].concat())?
        .ok_or(ApiError::Unauthorized)?;
    let user_role: Role = bincode::deserialize(&user_bytes).unwrap();
    if !user_role.can_write() {
        return abort(ApiError::Unauthorized);
    }

    Ok(())
}

async fn share(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: PermissionPair = serde_json::from_slice(&entire_body)?;

    if let Role::Owner = json.role {
        return Err(ApiError::Unauthorized);
    }

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref emails,
            ref user_to_album,
            ref album_to_user,
            ref albums,
            ..
        } = parts.data().unwrap();

        let album_id = parts.param("albumId").unwrap();

        test_logged_in(sessions, key)?;

        (emails, user_to_album, album_to_user, albums).transaction(
            |(emails, user_to_album, album_to_user, albums)| {
                // Test that the album exists so that albums that are being deleted
                // can't be shared
                albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;

                test_user_can_write(user_to_album, user_id, album_id)?;

                let target_user_id = emails.get(&*json.email)?.ok_or(ApiError::NotFound)?;

                let role_bytes = bincode::serialize(&json.role).unwrap();

                let prev_role_bytes = user_to_album
                    .insert([target_user_id.as_ref(), b".", album_id.as_bytes()].concat(), role_bytes)?;
                album_to_user.insert([album_id.as_bytes(), b".", target_user_id.as_ref()].concat(), b"")?;

                // Check to make sure that we didn't just modify the sharing permissions
                // for the owner of the album
                if let Some(prev_role_bytes) = prev_role_bytes {
                    let prev_role: Role = bincode::deserialize(&prev_role_bytes).unwrap();
                    if let Role::Owner = prev_role {
                        return abort(ApiError::BadRequest);
                    }
                }

                Ok(())
            },
        )?;

        respond_ok_empty()
    })
}

async fn unshare(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: Key = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref emails,
            ref user_to_album,
            ref album_to_user,
            ref files,
            ref inclusions,
            ref albums,
            ref fragments,
            ..
        } = parts.data().unwrap();

        let album_id = parts.param("albumId").unwrap();

        test_logged_in(sessions, key)?;

        (emails, user_to_album, album_to_user, inclusions, files, albums, fragments).transaction(
            |(emails, user_to_album, album_to_user, inclusions, files, albums, fragments)| {
                let target_user_id = emails.get(&*json.key)?.ok_or(ApiError::NotFound)?;

                // Users can remove themselves from an album if they want to
                if &target_user_id != user_id.as_bytes() {
                    test_user_can_write(user_to_album, user_id, album_id)?;
                }

                // Return if the user is already removed
                let role_bytes =
                    match user_to_album.remove([target_user_id.as_ref(), b".", album_id.as_bytes()].concat())? {
                        Some(x) => x,
                        None => return Ok(()),
                    };
                album_to_user.remove([album_id.as_bytes(), b".", target_user_id.as_ref()].concat())?;

                // Fail if someone tries to remove the owner
                let role: Role = bincode::deserialize(&role_bytes).unwrap();
                if let Role::Owner = role {
                    return abort(ApiError::BadRequest);
                }

                let album_bytes = albums.get(album_id)?.ok_or(ApiError::Unauthorized)?;
                let mut album: Album = bincode::deserialize(&album_bytes).unwrap();

                let mut e = Engine::new(&album_id, &mut album, fragments)?;

                // Remove all files that the target has added to the album. A user must be
                // able to see all of albums that their photos are in.
                for file_id in e.list_file_ids()? {
                    if let Some(file_bytes) = files.get(&file_id)? {
                        let file: File = bincode::deserialize(&file_bytes).unwrap();

                        if file.owner_id.as_bytes() == target_user_id.as_ref() {
                            let inclusion = [&file_id, ".", album_id].concat();
                            inclusions.remove(inclusion.as_bytes())?;

                            e.remove(&file_id, &file)?;
                        }
                    }
                }

                e.commit()?;

                Ok(())
            },
        )?;

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
            ref album_to_user,
            ref user_to_album,
            ref users,
            ..
        } = parts.data().unwrap();

        let album_id = parts.param("albumId").unwrap();

        test_logged_in(sessions, key)?;
        user_to_album
            .get([user_id, ".", album_id].concat())?
            .ok_or(ApiError::Unauthorized)?;

        let mut user_ids = vec![];
        for entry in album_to_user.scan_prefix([album_id, "."].concat()) {
            let (key, _) = entry?;
            let (_, user_id) = std::str::from_utf8(&key)
                .unwrap()
                .split_once(".")
                .unwrap();
            user_ids.push(user_id.to_string());
        }

        let mut pairs: Vec<PermissionPair<'static, '_>> = vec![];
        for user_id in user_ids {
            let key = [user_id.as_str(), ".", album_id].concat();
            if let Some(role_bytes) = user_to_album.get(key)? {
                if let Some(user_bytes) = users.get(&user_id)? {
                    let role: Role = bincode::deserialize(&role_bytes).unwrap();
                    let user: User = bincode::deserialize(&user_bytes).unwrap();

                    pairs.push(PermissionPair {
                        email: Cow::Owned(user.email.to_string()),
                        user_id: Some(Cow::from(user_id)),
                        role: role,
                    });
                }
            }
        }

        respond_ok(pairs)
    })
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/", share)
        .delete("/", unshare)
        .get("/", list)
        .build()
        .unwrap()
}
