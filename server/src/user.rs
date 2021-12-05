use crate::{
    delete,
    common::{
        join, new_id, require_key, respond_ok, respond_ok_empty, test_logged_in, AppState, User,
    },
    error::{ApiError, ApiResult},
};
use hyper::{Body, Request, Response};
use rand::{thread_rng, Rng};
use routerify::ext::RequestExt;
use routerify::Router;
use routerify_query::RequestQueryExt;
use sled::Transactional;
use std::borrow::Cow;
use tokio::task::block_in_place;
use wire::{Key, SessionList, UserDetails, ChangePassword};

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

async fn delete(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    block_in_place(|| {
        let state = parts.data().unwrap();
        let AppState {
            ref sessions,
            ..
        } = state;

        sessions.get(key)?.ok_or(ApiError::Unauthorized)?;

        delete::Command::User(user_id).run(state)?;

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

        test_logged_in(sessions, key)?;

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

        test_logged_in(sessions, key)?;

        for maybe_pair in sessions.scan_prefix(to_remove.as_bytes()) {
            let (key, _) = maybe_pair?;
            sessions.remove(key)?;
        }

        respond_ok_empty()
    })
}

async fn list_emails(req: Request<Body>) -> ApiResult<Response<Body>> {
    let prefix = req.query("prefix")
        .map(|s| s.as_str())
        .unwrap_or("")
        .to_owned();
    let skip = req.query("skip")
        .map(|s| s.parse::<usize>().ok())
        .unwrap_or(Some(0))
        .ok_or(ApiError::BadRequest)?;
    let take = req.query("take")
        .map(|s| s.parse::<usize>().ok())
        .unwrap_or(Some(usize::MAX))
        .ok_or(ApiError::BadRequest)?;

    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;

    block_in_place(|| {
        let AppState {
            ref sessions,
            ref emails,
            ..
        } = parts.data().unwrap();

        test_logged_in(sessions, key)?;

        let mut email_list = vec![];
        for entry in emails.scan_prefix(prefix).skip(skip).take(take) {
            let (key, _) = entry?;
            let email = std::str::from_utf8(&key).unwrap();
            email_list.push(email.to_owned());
            println!("email={}", email);
        }

        respond_ok(email_list)
    })
}

async fn change_password(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;
    let (user_id, _) = key.split_once('.').ok_or(ApiError::BadRequest)?;

    let entire_body = join(body).await?;
    let json: ChangePassword = serde_json::from_slice(&entire_body)?;

    block_in_place(|| {
        let AppState {
            ref users,
            ref sessions,
            ref argon_config,
            ..
        } = parts.data().unwrap();

        sessions.get(key)?.ok_or(ApiError::Unauthorized)?;

        users.transaction(|users| {
            let user_bytes = users.get(user_id)?.unwrap();
            let mut user: User = bincode::deserialize(&user_bytes).unwrap();
            
            verify_password(&user.password, &json.old_password)?;

            let hash = hash_password(json.new_password.as_bytes(), argon_config)?;
            user.password = &hash;

            let user_bytes = bincode::serialize(&user).unwrap();
            users.insert(user_id.as_bytes(), user_bytes)?;

            Ok(())
        })?;

        for entry in sessions.scan_prefix([user_id.as_bytes(), b"."].concat()) {
            let (key, _) = entry?;
            sessions.remove(key)?;
        }

        respond_ok_empty()
    })
}

pub fn router() -> Router<Body, ApiError> {
    Router::builder()
        .post("/", create)
        .delete("/", delete)
        .get("/emails", list_emails)
        .post("/auth", login)
        .put("/auth", change_password)
        .get("/auth", sessions)
        .delete("/auth", logout)
        .build()
        .unwrap()
}
