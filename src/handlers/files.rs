use crate::downloads::DownloadState;
use crate::state::AppState;
use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{
        HeaderValue,
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap},
    },
    response::IntoResponse,
};
use futures::stream::StreamExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::path::Path as StdPath;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::{error, info};
use uuid::Uuid;

pub async fn download_file(
    Path(path): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let file_path = state.settings.games_dir.join(&path);

    if !file_path.starts_with(&state.settings.games_dir) {
        return Err((axum::http::StatusCode::FORBIDDEN, "Forbidden"));
    }

    let file = match File::open(&file_path).await {
        Ok(f) => f,
        Err(e) => {
            error!("File download failed: {} (Path: {:?})", e, file_path);
            return Err((axum::http::StatusCode::NOT_FOUND, "File not found"));
        }
    };

    let metadata = file.metadata().await.unwrap();
    let total_size = metadata.len();
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();

    let download_id = Uuid::new_v4().to_string();
    info!("Starting download: {} (ID: {})", filename, download_id);

    {
        let mut downloads = state.downloads.lock().unwrap();
        downloads.insert(
            download_id.clone(),
            DownloadState {
                id: download_id.clone(),
                filename: filename.clone(),
                total_size,
                bytes_sent: 0,
                speed: 0,
            },
        );
    }

    let stream = ReaderStream::new(file);
    let downloads_clone = state.downloads.clone();
    let id_clone = download_id.clone();

    let stream = stream.map(move |chunk: Result<Bytes, std::io::Error>| {
        if let Ok(bytes) = &chunk {
            let len = bytes.len() as u64;
            if let Ok(mut downloads) = downloads_clone.lock()
                && let Some(download) = downloads.get_mut(&id_clone)
            {
                download.bytes_sent += len;
            }
        }
        chunk
    });

    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();

    // Determine content type based on extension
    let content_type = match StdPath::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };

    headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());

    if content_type == "application/octet-stream"
        && let Ok(val) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
    {
        headers.insert(CONTENT_DISPOSITION, val);
    }

    if let Ok(val) = HeaderValue::from_str(&total_size.to_string()) {
        headers.insert(CONTENT_LENGTH, val);
    }

    Ok((headers, body))
}

pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}
