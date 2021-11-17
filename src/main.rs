mod common;
mod error;
mod file;
mod user;
mod wire;

use common::AppState;
use error::ApiError;
use hyper::{Body, Response, Server, StatusCode};
use routerify::{Router, RouterService};
use std::net::SocketAddr;

async fn handle_error(error: routerify::RouteError) -> Response<Body> {
    let api_error = error.downcast::<ApiError>().unwrap();

    match api_error.as_ref() {
        ApiError::Unauthorized => Response::builder().status(StatusCode::UNAUTHORIZED),
        ApiError::NotFound => Response::builder().status(StatusCode::NOT_FOUND),
        ApiError::Hyper(_)
        | ApiError::Sled(_)
        | ApiError::Argon(_)
        | ApiError::IO(_)
        | ApiError::Vips(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR),
        ApiError::BadRequest | ApiError::Json(_) | ApiError::EmailTaken => {
            Response::builder().status(StatusCode::BAD_REQUEST)
        }
    }
    .body(Body::from(api_error.to_string()))
    .unwrap()
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

#[tokio::main]
async fn main() {
    // Set up libvips for use
    let vips = libvips::VipsApp::new("vips", true).unwrap();
    vips.concurrency_set(1);

    let state = AppState::new();
    state.create_dirs().expect("Couldn't set up directories");

    let removed = file::clean_files(&state).await.unwrap();
    println!("Removed {} files", removed);

    let router = Router::builder()
        // Provide app state to routes
        .data(state)
        // Routes
        .scope("/user", user::router())
        .scope("/file", file::router())
        // Not found for invalid paths
        .any(|_| async { Err(ApiError::NotFound) })
        .err_handler(handle_error)
        .build()
        .unwrap();

    let service = RouterService::new(router).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let server = Server::bind(&addr)
        .serve(service)
        .with_graceful_shutdown(shutdown_signal());

    println!("Running on: {}", addr);
    if let Err(err) = server.await {
        eprintln!("Server error: {}", err);
    }
    println!("\rShutting down...");

    drop(vips);
}
