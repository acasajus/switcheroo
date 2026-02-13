use crate::config::Settings;
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose};
use dav_server::{DavHandler, localfs::LocalFs};

pub fn create_dav_handler(settings: &Settings) -> DavHandler {
    let dir = settings.games_dir.clone();
    DavHandler::builder()
        .filesystem(LocalFs::new(dir, false, false, false))
        .locksystem(dav_server::memls::MemLs::new())
        .strip_prefix("/dav")
        .build_handler()
}

pub async fn webdav_handler(
    settings: Settings,
    dav_handler: DavHandler,
    req: Request<Body>,
) -> impl IntoResponse {
    tracing::info!("WebDAV Request: {} {}", req.method(), req.uri());

    // Check authentication if configured
    let (username, password) = match (&settings.webdav_username, &settings.webdav_password) {
        (Some(u), Some(p)) => (u, p),
        _ => return dav_handler.handle(req).await.into_response(),
    };

    let unauthorized = || {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "Basic realm=\"Switcheroo WebDAV\"")
            .body(Body::empty())
            .unwrap()
            .into_response()
    };

    let header_val = match req.headers().get("Authorization") {
        Some(h) => h,
        None => return unauthorized(),
    };

    let auth_str = match header_val.to_str() {
        Ok(s) => s,
        Err(_) => return unauthorized(),
    };

    let token = match auth_str.strip_prefix("Basic ") {
        Some(t) => t,
        None => return unauthorized(),
    };

    let decoded = match general_purpose::STANDARD.decode(token) {
        Ok(d) => d,
        Err(_) => return unauthorized(),
    };

    let creds = match String::from_utf8(decoded) {
        Ok(c) => c,
        Err(_) => return unauthorized(),
    };

    let expected = format!("{}:{}", username, password);
    if creds != expected {
        return unauthorized();
    }

    dav_handler.handle(req).await.into_response()
}
