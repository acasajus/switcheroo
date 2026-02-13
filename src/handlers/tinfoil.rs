use crate::handlers::files::encode_path;
use crate::state::AppState;
use crate::tinfoil;
use axum::{
    Json,
    body::Body,
    extract::State,
    http::header::{CONTENT_TYPE, HeaderMap},
    response::IntoResponse,
};
use tracing::error;

pub async fn tinfoil_index(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let games = state.games.lock().unwrap();

    // Determine host from header or fallback to internal config
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .map(|h| format!("http://{}", h))
        .unwrap_or(state.host_url.clone());

    let files: Vec<serde_json::Value> = games
        .iter()
        .map(|game| {
            let encoded_path = encode_path(&game.relative_path);
            let url = format!("{}/files/{}", host, encoded_path);

            serde_json::json!({
                "url": url,
                "size": game.size,
            })
        })
        .collect();

    let shop_json = serde_json::json!({
        "files": files,
        "success": "The index was generated successfully.",
    });

    if state.settings.tinfoil_encrypt {
        let json_bytes = serde_json::to_vec(&shop_json).unwrap();
        match tinfoil::encrypt_shop(&json_bytes) {
            Ok(encrypted) => {
                return (
                    [(CONTENT_TYPE, "application/octet-stream")],
                    Body::from(encrypted),
                )
                    .into_response();
            }
            Err(e) => {
                error!("Encryption failed: {}", e);
            }
        }
    }

    Json(shop_json).into_response()
}
