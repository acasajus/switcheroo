use crate::config::Settings;
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose};
use dav_server::{DavHandler, localfs::LocalFs};
use std::sync::Arc;

#[derive(Clone)]
pub struct WebDavState {
    handler: DavHandler,
    settings: Settings,
}

impl WebDavState {
    pub fn new(settings: Settings) -> Self {
        let dir = settings.games_dir.clone();
        let handler = DavHandler::builder()
            .filesystem(LocalFs::new(dir, false, false, false))
            .locksystem(dav_server::memls::MemLs::new())
            .strip_prefix("/dav")
            .build_handler();

        Self { handler, settings }
    }
}

pub async fn webdav_handler(
    State(state): State<Arc<WebDavState>>,
    req: Request<Body>,
) -> impl IntoResponse {
    // Check authentication if configured
    if let (Some(username), Some(password)) = (
        &state.settings.webdav_username,
        &state.settings.webdav_password,
    ) {
        let auth_header = req.headers().get("Authorization");

                let authorized = if let Some(header_val) = auth_header {

                    if let Ok(auth_str) = header_val.to_str() {

                        if let Some(token) = auth_str.strip_prefix("Basic ") {

                            if let Ok(decoded) = general_purpose::STANDARD.decode(token) {

                                if let Ok(creds) = String::from_utf8(decoded) {

                                     let expected = format!("{}:{}", username, password);

                                     creds == expected

                                } else { false }

                            } else { false }

                        } else { false }

                    } else { false }

                } else { false };

        

        if !authorized {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("WWW-Authenticate", "Basic realm=\"Switcheroo WebDAV\"")
                .body(Body::empty())
                .unwrap();
        }
    }

    state.handler.handle(req).await.into_response()
}
