mod config;
mod downloads;
mod metadata;
mod scanner;
mod webdav;

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{
        HeaderValue,
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap},
    },
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
    routing::{any, get},
};
use config::Settings;
use downloads::{DownloadState, Downloads};
use futures::stream::{Stream, StreamExt};
use local_ip_address::local_ip;
use notify::{
    Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use scanner::{Game, process_entry};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::fs::File;
use tokio::sync::broadcast;
use tokio_util::io::ReaderStream;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{Level, debug, error, info};
use uuid::Uuid;
use walkdir::WalkDir;
use webdav::WebDavState;

#[derive(Clone)]
struct AppState {
    games: Arc<Mutex<Vec<Game>>>,
    settings: Settings,
    host_url: String,
    downloads: Downloads,
    tx: broadcast::Sender<String>,
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file if it exists
    dotenvy::dotenv().ok();

    // Load configuration
    let settings = Settings::new().expect("Failed to load configuration");

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(&settings.log_level)
        .init();

    info!("Starting Switcheroo...");
    debug!("Configuration loaded: {:?}", settings);

    // Ensure the games directory exists
    if !settings.games_dir.exists() {
        info!(
            "Games directory not found, creating: {:?}",
            settings.games_dir
        );
        std::fs::create_dir_all(&settings.games_dir).expect("Failed to create games directory");
    }

    // Ensure the data directory exists
    let images_dir = settings.data_dir.join("images");
    if !images_dir.exists() {
        info!("Images directory not found, creating: {:?}", images_dir);
        std::fs::create_dir_all(&images_dir).expect("Failed to create images directory");
    }

    let games = Arc::new(Mutex::new(Vec::new()));

    let local_ip = local_ip().unwrap_or("127.0.0.1".parse().unwrap());
    let host_url = format!("http://{}:{}", local_ip, settings.server_port);

    let downloads = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _rx) = broadcast::channel(100);

    let state = AppState {
        games: games.clone(),
        settings: settings.clone(),
        host_url: host_url.clone(),
        downloads: downloads.clone(),
        tx: tx.clone(),
    };

    // Image Download Channel
    let (img_tx, mut img_rx) = tokio::sync::mpsc::channel::<(String, std::path::PathBuf)>(100);

    // Image Downloader Task
    let games_img = games.clone();
    let data_dir_img = settings.data_dir.clone();
    let tx_img = tx.clone();

    tokio::spawn(async move {
        while let Some((title_id, game_path)) = img_rx.recv().await {
            // Determine target image path in data_dir/images
            let target_path = data_dir_img
                .join("images")
                .join(format!("{}.jpg", title_id));

            // Check if already exists (should happen in process_entry, but good double check)
            if target_path.exists() {
                continue;
            }

            // Wait a bit to avoid rate limits if many
            // tokio::time::sleep(Duration::from_millis(500)).await;

            let saved_path = match metadata::download_image(&title_id, target_path).await {
                Some(p) => p,
                None => continue,
            };

            // Update state
            let mut games = games_img.lock().unwrap();
            let game = match games.iter_mut().find(|g| g.path == game_path) {
                Some(g) => g,
                None => continue,
            };

            // Determine extension from saved path
            let ext = saved_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("jpg");
            let rel_url = format!("/images/{}.{}", title_id, ext);

            game.image_url = Some(rel_url);

            // Notify frontend to refresh games list
            let _ = tx_img.send(
                serde_json::json!({
                    "type": "scan",
                    "status": "image_updated",
                    "count": 0 // Dummy
                })
                .to_string(),
            );
        }
    });

    // Background Game Scanning Task
    let games_dir = settings.games_dir.clone();
    let data_dir_scan = settings.data_dir.clone();
    let games_clone = games.clone();
    let tx_scan = tx.clone();
    let img_tx_scan = img_tx.clone();

    tokio::task::spawn_blocking(move || {
        info!("Starting background game scan in: {:?}", games_dir);
        let start_time = std::time::Instant::now();

        // Notify start
        let _ = tx_scan.send(
            serde_json::json!({
                "type": "scan",
                "status": "scanning",
                "count": 0
            })
            .to_string(),
        );

        let mut batch = Vec::new();
        let mut total_count = 0;

        for entry in WalkDir::new(&games_dir).into_iter().filter_map(|e| e.ok()) {
            let game = match process_entry(entry.path(), &games_dir, &data_dir_scan) {
                Some(g) => g,
                None => continue,
            };

            // Queue image download if needed
            if game.image_url.is_none()
                && let Some(ref tid) = game.title_id
            {
                let _ = img_tx_scan.blocking_send((tid.clone(), game.path.clone()));
            }

            batch.push(game);
            total_count += 1;

            // Update batch every 50 items to keep UI responsive but performant
            if batch.len() >= 50 {
                let mut g_lock = games_clone.lock().unwrap();
                g_lock.extend(batch.drain(..));
                drop(g_lock); // Release lock immediately

                let _ = tx_scan.send(
                    serde_json::json!({
                        "type": "scan",
                        "status": "scanning",
                        "count": total_count
                    })
                    .to_string(),
                );
            }
        }

        // Flush remaining
        if !batch.is_empty() {
            let mut g_lock = games_clone.lock().unwrap();
            g_lock.extend(batch);
        }

        let duration = start_time.elapsed();
        info!(
            "Scan complete. Indexed {} games in {:.2?}.",
            total_count, duration
        );

        let _ = tx_scan.send(
            serde_json::json!({
                "type": "scan",
                "status": "complete",
                "count": total_count
            })
            .to_string(),
        );
    });
    // Background task to calculate speeds and broadcast updates
    let downloads_clone = downloads.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_bytes_map: HashMap<String, u64> = HashMap::new();

        loop {
            interval.tick().await;
            let mut downloads = downloads_clone.lock().unwrap();
            let mut current_ids = Vec::new();

            for (id, download) in downloads.iter_mut() {
                current_ids.push(id.clone());
                let last = last_bytes_map.get(id).cloned().unwrap_or(0);
                let current = download.bytes_sent;

                if current >= last {
                    download.speed = current - last;
                }

                last_bytes_map.insert(id.clone(), current);
            }

            // Clean up finished downloads from local map
            last_bytes_map.retain(|k, _| current_ids.contains(k));

            if downloads.is_empty() {
                continue;
            }

            let data_json = match serde_json::to_value(&*downloads) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let msg = serde_json::json!({
                "type": "downloads",
                "data": data_json
            })
            .to_string();
            let _ = tx_clone.send(msg);
        }
    });

    // File Watcher Task
    let games_dir_watch = settings.games_dir.clone();
    let data_dir_watch = settings.data_dir.clone();
    let games_watch = games.clone();
    let tx_watch = tx.clone();
    let img_tx_watch = img_tx.clone(); // If we want to queue images on create

    tokio::task::spawn_blocking(move || {
        let (std_tx, std_rx) = channel();

        let watcher = RecommendedWatcher::new(std_tx, Config::default());
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create watcher: {:?}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&games_dir_watch, RecursiveMode::Recursive) {
            error!("Failed to watch games directory: {:?}", e);
            return;
        }

        info!("File watcher started for: {:?}", games_dir_watch);

        for res in std_rx {
            let event = match res {
                Ok(e) => e,

                Err(e) => {
                    error!("Watch error: {:?}", e);

                    continue;
                }
            };

            match event.kind {
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                    // Renaming: paths[0] is from, paths[1] is to

                    if event.paths.len() != 2 {
                        continue;
                    }

                    let from = &event.paths[0];

                    let to = &event.paths[1];

                    let mut games = games_watch.lock().unwrap();

                    // Remove old

                    if let Some(idx) = games.iter().position(|g| g.path == *from) {
                        games.remove(idx);

                        let _ = tx_watch.send(
                            serde_json::json!({

                               "type": "scan",

                               "status": "remove",

                               "path": from

                            })
                            .to_string(),
                        );
                    }

                    drop(games); // release lock before processing new

                    // Add new

                    let game = match process_entry(to, &games_dir_watch, &data_dir_watch) {
                        Some(g) => g,

                        None => continue,
                    };

                    // Queue image

                    if game.image_url.is_none()
                        && let Some(ref tid) = game.title_id
                    {
                        let _ = img_tx_watch.blocking_send((tid.clone(), game.path.clone()));
                    }

                    let mut games = games_watch.lock().unwrap();

                    games.push(game.clone());

                    let _ = tx_watch.send(
                        serde_json::json!({

                           "type": "scan",

                           "status": "update",

                           "game": game

                        })
                        .to_string(),
                    );
                }

                EventKind::Create(_) | EventKind::Modify(_) => {
                    for path in event.paths {
                        if !path.is_file() {
                            continue;
                        }

                        let game = match process_entry(&path, &games_dir_watch, &data_dir_watch) {
                            Some(g) => g,

                            None => continue,
                        };

                        info!("Watcher: Game detected/updated: {}", game.name);

                        // Queue image download if new and missing

                        if game.image_url.is_none()
                            && let Some(ref tid) = game.title_id
                        {
                            let _ = img_tx_watch.blocking_send((tid.clone(), game.path.clone()));
                        }

                        let mut games = games_watch.lock().unwrap();

                        if let Some(idx) = games.iter().position(|g| g.path == game.path) {
                            games[idx] = game.clone();
                        } else {
                            games.push(game.clone());
                        }

                        let _ = tx_watch.send(
                            serde_json::json!({

                               "type": "scan",

                               "status": "update",

                               "game": game

                            })
                            .to_string(),
                        );
                    }
                }

                EventKind::Remove(_) => {
                    for path in event.paths {
                        let mut games = games_watch.lock().unwrap();

                        let idx = match games.iter().position(|g| g.path == path) {
                            Some(i) => i,

                            None => continue,
                        };

                        let removed = games.remove(idx);

                        info!("Watcher: Game removed: {}", removed.name);

                        let _ = tx_watch.send(
                            serde_json::json!({

                               "type": "scan",

                               "status": "remove",

                               "path": path

                            })
                            .to_string(),
                        );
                    }
                }

                _ => {}
            }
        }
    });

    let frontend_dist = "frontend/dist";

    // WebDAV State
    let webdav_state = Arc::new(WebDavState::new(settings.clone()));

    // WebDAV Router
    let dav_router = Router::new()
        .route("/", any(webdav::webdav_handler))
        .route("/{*path}", any(webdav::webdav_handler))
        .with_state(webdav_state);

    let mut app = Router::new()
        .route("/api/games", get(list_games))
        .route("/api/info", get(server_info))
        .route("/tinfoil", get(tinfoil_index))
        .route("/tinfoil/", get(tinfoil_index))
        .route("/tinwoo", get(tinfoil_index))
        .route("/tinwoo/", get(tinfoil_index))
        .route("/dbi", get(dbi_index))
        .route("/dbi/", get(dbi_index))
        .route("/dbi/{*path}", get(download_file))
        .route("/events", get(sse_handler))
        .route("/files/{*path}", get(download_file))
        .nest_service("/images", ServeDir::new(settings.data_dir.join("images")));

    if settings.webdav_enabled {
        app = app.nest("/dav", dav_router);
    }

    let app = app
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
        // Fallback service to handle frontend assets and SPA routing
        .fallback_service(
            ServeDir::new(frontend_dist)
                .fallback(ServeFile::new(format!("{}/index.html", frontend_dist))),
        );

    let port = settings.server_port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on http://{}", addr);
    info!("Network address: {}", host_url);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn server_info(State(state): State<AppState>) -> Json<serde_json::Value> {
    let ips = local_ip_address::list_afinet_netifas()
        .map(|list| {
            list.into_iter()
                .filter(|(_, ip)| ip.is_ipv4() && !ip.is_loopback())
                .map(|(_, ip)| ip.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let webdav_auth =
        state.settings.webdav_username.is_some() && state.settings.webdav_password.is_some();

    Json(serde_json::json!({
        "ips": ips,
        "port": state.settings.server_port,
        "webdav_enabled": state.settings.webdav_enabled,
        "webdav_auth": webdav_auth
    }))
}

async fn list_games(State(state): State<AppState>) -> Json<Vec<Game>> {
    let games = state.games.lock().unwrap();
    Json(games.clone())
}

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(|msg| match msg {
        Ok(msg) => Ok(Event::default().data(msg)),
        Err(_) => Ok(Event::default().comment("keepalive")),
    });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn download_file(
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
    let content_type = match std::path::Path::new(&filename)
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

    // Only set attachment disposition if it's not an image (or if we want to force download for everything else)
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

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

async fn tinfoil_index(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
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

    Json(serde_json::json!({
        "files": files,
        "success": "The index was generated successfully.",
    }))
}

async fn dbi_index(State(state): State<AppState>) -> axum::response::Html<String> {
    let games = state.games.lock().unwrap();

    let mut html = String::from(
        "<!DOCTYPE html><html><head><title>DBI Index</title></head><body><h1>Index of /</h1><ul>",
    );

    for game in games.iter() {
        // Use relative paths for DBI so it treats files as being in the current "directory"
        // This requires mounting the download handler at /dbi/{*path} as well.
        let url = encode_path(&game.relative_path);
        let name = game.name.clone();

        html.push_str(&format!("<li><a href=\"{}\">{}</a></li>", url, name));
    }

    html.push_str("</ul></body></html>");

    axum::response::Html(html)
}
