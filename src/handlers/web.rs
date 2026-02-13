use axum::{
    http::header::CONTENT_TYPE,
    response::IntoResponse,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
pub struct Assets;

pub async fn static_handler(uri: axum::http::Uri) -> axum::response::Response {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == "index.html" {
        return index_handler().await;
    }

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            // For SPA, redirect unknown paths to index.html
            index_handler().await
        }
    }
}

pub async fn index_handler() -> axum::response::Response {
    match Assets::get("index.html") {
        Some(content) => ([(CONTENT_TYPE, "text/html")], content.data).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            "index.html not found in embedded assets",
        )
            .into_response(),
    }
}
