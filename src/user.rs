use crate::common::{join, require_key, ApiError, ApiResult, AppState};
use hyper::{Body, Request, Response, StatusCode};
use rand::{thread_rng, Rng};
use routerify::ext::RequestExt;
use routerify::Router;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokio::task::block_in_place;

#[derive(Deserialize)]
struct CreateReq<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Serialize, Deserialize)]
struct KeyReqRes<'a> {
    key: &'a str,
}

#[derive(Serialize)]
struct SessionsRes<'a> {
    key_prefixes: Vec<&'a str>,
}

async fn create(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let entire_body = join(body).await?;
    let json: CreateReq = serde_json::from_slice(&entire_body)?;

    let app_state = parts.data::<AppState>().unwrap();
    let db = app_state.pool.get()?;

    block_in_place(move || {
        let salt: [u8; 32] = thread_rng().gen();
        let hash = argon2::hash_encoded(json.password.as_bytes(), &salt, &app_state.argon_config)?;

        let user_id: i64 = thread_rng().gen();

        db.execute(
            "INSERT OR IGNORE INTO
            users (user_id, email, password)
            VALUES (?, ?, ?)",
            params![user_id, json.email, hash],
        )?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap())
    })
}

async fn login(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let entire_body = join(body).await?;
    let json: CreateReq = serde_json::from_slice(&entire_body)?;

    let key_bytes: [u8; 32] = thread_rng().gen();
    let key = base64::encode_config(&key_bytes, base64::URL_SAFE_NO_PAD);

    let app_state = parts.data::<AppState>().unwrap();
    let mut db = app_state.pool.get()?;
    let tx = db.transaction()?;

    block_in_place(move || {
        let maybe: Option<(String, i64)> = tx
            .query_row(
                "SELECT password, user_id FROM users WHERE email = ?",
                params![json.email],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if let Some((hash, user_id)) = maybe {
            if argon2::verify_encoded(&hash, json.password.as_bytes())? {
                tx.execute(
                    "INSERT INTO sessions (key, user_id)
                    VALUES (?, ?)",
                    params![key, user_id],
                )?;

                tx.commit()?;

                let response = serde_json::to_string(&KeyReqRes { key: &key })?;

                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(response))
                    .unwrap());
            }
        }

        Err(ApiError::Unauthorized)
    })
}

async fn sessions(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, _) = req.into_parts();

    let key = require_key(&parts)?;

    let app_state = parts.data::<AppState>().unwrap();
    let mut db = app_state.pool.get()?;
    let tx = db.transaction()?;

    block_in_place(move || {
        let user_id: i64 = tx
            .query_row(
                "SELECT user_id FROM sessions WHERE key = ?",
                params![key],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ApiError::Unauthorized)?;

        let mut statement = tx.prepare("SELECT key FROM sessions WHERE user_id = ?")?;

        let keys = statement
            .query_map(params![user_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        let prefixes = keys.iter().map(|s| &s[..8]).collect();

        let json = serde_json::to_string(&SessionsRes {
            key_prefixes: prefixes,
        })?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(json))
            .unwrap())
    })
}

async fn logout(req: Request<Body>) -> ApiResult<Response<Body>> {
    let (parts, body) = req.into_parts();

    let key = require_key(&parts)?;

    let entire_body = join(body).await?;
    let json: KeyReqRes = serde_json::from_slice(&entire_body)?;

    // Check to make sure that no injections into the glob are possible
    // I don't think that this is a problem, but I want to be safe
    if !json.key.chars().all(char::is_alphanumeric) {
        return Err(ApiError::BadRequest);
    }

    let app_state = parts.data::<AppState>().unwrap();
    let mut db = app_state.pool.get()?;
    let tx = db.transaction()?;

    block_in_place(move || {
        let user_id: i64 = tx
            .query_row(
                "SELECT user_id FROM sessions WHERE key = ?",
                params![key],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ApiError::Unauthorized)?;

        tx.execute(
            "DELETE FROM sessions WHERE
            user_id = ? AND key GLOB ? || '*'",
            params![user_id, json.key],
        )?;

        tx.commit()?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap())
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
