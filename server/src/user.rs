use crate::{
    common::{join, new_id, require_key, respond_ok, respond_ok_empty, AppState, User},
    error::{ApiError, ApiResult},
};
use hyper::{Body, Request, Response};
use rand::{thread_rng, Rng};
use routerify::ext::RequestExt;
use routerify::Router;
use sled::Transactional;
use std::borrow::Cow;
use tokio::task::block_in_place;
use wire::{Key, SessionList, UserDetails};

const USER_ID_BYTES: usize = 8;
const SESSION_KEY_BYTES: usize = 32;

fn hash_password(password: &[u8], config: &argon2::Config) -> ApiResult<String> {
    let salt: [u8; 32] = thread_rng().gen();
    let hash = argon2::hash_encoded(password, &salt, config)?;

    Ok(hash)
}

fn verify_password(hash: &str, password: &str) -> ApiResult<()> {
    if !argon2::verify_encoded(hash, password.as_bytes())? {
        return Err(ApiError::Unauthorized.into());
    }

    Ok(())
}

async fn create(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let entire_body = join(body).await?;
    let json: UserDetails = serde_json::from_slice(&entire_body)?;

    block_in_place(move || {
        let AppState {
            ref users,
            ref emails,
            ref argon_config,
            ..
        } = parts.data().unwrap();

        let user_id = new_id(USER_ID_BYTES);
        let hash = hash_password(json.password.as_bytes(), argon_config)?;

        let user = User {
            email: &json.email,
            password: &hash,
        };

        (users, emails).transaction(|(users, emails)| {
            if emails.insert(&*json.email, user_id.as_bytes())?.is_some() {
                return Err(ApiError::EmailTaken.into());
            }

            users.insert(user_id.as_bytes(), bincode::serialize(&user).unwrap())?;

            Ok(())
        })?;

        respond_ok_empty()
    })
}

async fn login(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let entire_body = join(body).await?;
    let json: UserDetails = serde_json::from_slice(&entire_body)?;

    block_in_place(move || {
        let AppState {
            ref users,
            ref emails,
            ref sessions,
            ..
        } = parts.data().unwrap();

        let key = new_id(SESSION_KEY_BYTES);

        let extended_key = (users, emails, sessions).transaction(|(users, emails, sessions)| {
            let user_id = emails.get(&*json.email)?.ok_or(ApiError::Unauthorized)?;

            let user_bytes = users.get(&user_id)?.unwrap();
            let user: User = bincode::deserialize(&user_bytes).unwrap();

            verify_password(&user.password, &json.password)?;

            let extended_key = [user_id.as_ref(), b".", key.as_bytes()].concat();

            sessions.insert(extended_key.clone(), b"")?;

            Ok(extended_key)
        })?;

        respond_ok(Key {
            key: Cow::from(std::str::from_utf8(&extended_key).unwrap()),
        })
    })
}

async fn sessions(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;

    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    block_in_place(|| {
        let AppState { ref sessions, .. } = parts.data().unwrap();

        let mut prefixes = vec![];

        sessions.get(key)?.ok_or(ApiError::Unauthorized)?;

        for maybe_pair in sessions.scan_prefix(&user_id) {
            let (key, _) = maybe_pair?;
            let string = String::from(&std::str::from_utf8(key.as_ref()).unwrap()[..20]);
            prefixes.push(Cow::from(string));
        }

        respond_ok(SessionList {
            key_prefixes: prefixes,
        })
    })
}

async fn logout(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;

    let entire_body = join(body).await?;
    let json: Key = serde_json::from_slice(&entire_body)?;

    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;
    let (_, prefix) = json.key.split_once('.').ok_or(ApiError::BadRequest)?;
    let to_remove = [user_id, ".", prefix].concat();

    block_in_place(|| {
        let AppState { ref sessions, .. } = parts.data().unwrap();

        sessions.get(key)?.ok_or(ApiError::Unauthorized)?;

        for maybe_pair in sessions.scan_prefix(to_remove.as_bytes()) {
            let (key, _) = maybe_pair?;
            sessions.remove(key)?;
        }

        respond_ok_empty()
    })
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/create", create)
        .post("/login", login)
        .get("/sessions", sessions)
        .delete("/logout", logout)
        .build()
        .unwrap()
}