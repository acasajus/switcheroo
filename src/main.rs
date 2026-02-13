mod config;
mod downloads;
mod metadata;
mod scanner;
mod tinfoil;
mod webdav;

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, State, Request},
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
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::{Level, debug, error, info};
use uuid::Uuid;
use walkdir::WalkDir;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub games: Arc<Mutex<Vec<Game>>>,
    pub settings: Settings,
    pub host_url: String,
    pub downloads: Downloads,
    pub tx: broadcast::Sender<String>,
    pub metadata: Arc<tokio::sync::Mutex<crate::metadata::MetadataProvider>>,
    pub dav_handler: dav_server::DavHandler,
}



    

    async fn static_handler(uri: axum::http::Uri) -> axum::response::Response {



    

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



    

    



    

    async fn index_handler() -> axum::response::Response {



    

        match Assets::get("index.html") {



    

            Some(content) => ([(CONTENT_TYPE, "text/html")], content.data).into_response(),



    

            None => (



    

                axum::http::StatusCode::NOT_FOUND,



    

                "index.html not found in embedded assets",



    

            )



    

                .into_response(),



    

        }



    

    }



    

    

    

    

    

    

    

    async fn webdav_wrapper(

    

    

    

    

    

        State(state): State<AppState>,

    

    

    

    

    

        req: Request<Body>,

    

    

    

    

    

    ) -> impl IntoResponse {

    

    

    

    

    

        let method = req.method().clone();

    

    

    

    

    

        let uri = req.uri().clone();

    

    

    

    

    

        

    

    

    

    

    

        let mut response =

    

    

    

    

    

            webdav::webdav_handler(state.settings.clone(), state.dav_handler.clone(), req)

    

    

    

    

    

                .await

    

    

    

    

    

                .into_response();

    

    

    

    

    

    

    

    

    

    

    

        // Ensure WebDAV identification headers are ALWAYS present

    

    

    

    

    

        let headers = response.headers_mut();

    

    

    

    

    

        headers.insert("DAV", HeaderValue::from_static("1, 2"));

    

    

    

    

    

        headers.insert("MS-Author-Via", HeaderValue::from_static("DAV"));

    

    

    

    

    

    

    

    

    

    

    

        if !response.status().is_success() {

    

    

    

    

    

            debug!("WebDAV Response Error: {} {} -> {}", method, uri, response.status());

    

    

    

    

    

        } else {

    

    

    

    

    

            info!("WebDAV Request: {} {} -> {}", method, uri, response.status());

    

    

    

    

    

        }

    

    

    

    

    

    

    

    

    

    

    

        response

    

    

    

    

    

    }

    

    

    

    

    

    

    

    

    

    

    

    #[tokio::main]

    

    

    

    

    

    async fn main() {

    

    

    

    

    

        // ... (keep existing initialization code until router) ...

    

    

    

    

    

    

    

    

    

    

    

    

    

    

    

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



        



            let metadata = Arc::new(tokio::sync::Mutex::new(crate::metadata::MetadataProvider::new(



                settings.data_dir.clone(),



                settings.metadata_region.clone(),



                settings.metadata_language.clone(),



            ).await));



        



            // Async Metadata Initialization



            let metadata_init = metadata.clone();



            tokio::spawn(async move {



                let mut meta = metadata_init.lock().await;



                meta.init().await;



                info!("Metadata initialized and ready.");



            });



        



            let dav_handler = webdav::create_dav_handler(&settings);



        



            let state = AppState {



                games: games.clone(),



                settings: settings.clone(),



                host_url: host_url.clone(),



                downloads: downloads.clone(),



                tx: tx.clone(),



                metadata: metadata.clone(),



                dav_handler,



            };



        



            // Metadata Sync Task



            let metadata_clone = metadata.clone();



            let tx_sync = tx.clone();



            tokio::spawn(async move {



                let mut interval = tokio::time::interval(Duration::from_secs(24 * 3600)); // Every 24h



                loop {



                    interval.tick().await;



                    info!("Starting periodic metadata sync...");



                    let mut meta = metadata_clone.lock().await;



                    if let Err(e) = meta.sync().await {



                        error!("Failed to sync metadata: {}", e);



                    } else {



                        info!("Metadata sync complete.");



                        let _ = tx_sync.send(



                            serde_json::json!({



                                "type": "sync",



                                "status": "complete"



                            })



                            .to_string(),



                        );



                    }



                    drop(meta);



                }



            });



        



            // Background Game Scanning Task



            let games_dir = settings.games_dir.clone();



            let data_dir_scan = settings.data_dir.clone();



            let games_clone = games.clone();



            let tx_scan = tx.clone();



            let metadata_scan = metadata.clone();



        



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



        



                let handle = tokio::runtime::Handle::current();



                let meta_provider_guard = handle.block_on(metadata_scan.lock());



        



                for entry in WalkDir::new(&games_dir).into_iter().filter_map(|e| e.ok()) {



                    let game = match process_entry(entry.path(), &games_dir, &data_dir_scan, Some(&meta_provider_guard)) {



                        Some(g) => g,



                        None => continue,



                    };



        



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

            let metadata_watch = metadata.clone();

        

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

                            let handle = tokio::runtime::Handle::current();

                            let meta_provider = handle.block_on(metadata_watch.lock());

                            let game = match process_entry(to, &games_dir_watch, &data_dir_watch, Some(&meta_provider)) {

                                Some(g) => g,

        

                                None => continue,

                            };

        

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

        

                                let handle = tokio::runtime::Handle::current();

                                let meta_provider = handle.block_on(metadata_watch.lock());

                                let game = match process_entry(&path, &games_dir_watch, &data_dir_watch, Some(&meta_provider)) {

                                    Some(g) => g,

        

                                    None => continue,

                                };

        

                                info!("Watcher: Game detected/updated: {}", game.name);

        

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

        

    // --- Router Setup ---
    let app = create_app(state);

    let port = settings.server_port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on http://{}", addr);
    info!("Network address: {}", host_url);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

pub fn create_app(state: AppState) -> Router {
    let mut app = Router::new();

    // 1. WebDAV Routes (No global layers)
    if state.settings.webdav_enabled {
        app = app
            .route("/dav", any(webdav_wrapper))
            .route("/dav/", any(webdav_wrapper))
            .route("/dav/{*path}", any(webdav_wrapper));
    }

    // 2. API and Assets (With Layers)
    let main_routes = Router::new()
        .route("/api/games", get(list_games))
        .route("/api/info", get(server_info))
        .route("/api/sync", get(sync_metadata))
        .route("/tinfoil", get(tinfoil_index))
        .route("/tinfoil/", get(tinfoil_index))
        .route("/tinwoo", get(tinfoil_index))
        .route("/tinwoo/", get(tinfoil_index))
        .route("/dbi", get(dbi_index))
        .route("/dbi/", get(dbi_index))
        .route("/dbi/{*path}", get(download_file))
        .route("/events", get(sse_handler))
        .route("/files/{*path}", get(download_file))
        .nest_service(
            "/images",
            ServeDir::new(state.settings.data_dir.join("images")),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(CorsLayer::permissive());

    app.merge(main_routes)
        .with_state(state)
        .fallback(static_handler)
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

async fn sync_metadata(State(state): State<AppState>) -> Json<serde_json::Value> {
    info!("Manual metadata sync requested.");
    let metadata = state.metadata.clone();
    let tx = state.tx.clone();
    let games_dir = state.settings.games_dir.clone();
    let data_dir = state.settings.data_dir.clone();
    let games = state.games.clone();

    tokio::spawn(async move {
        {
            let mut meta = metadata.lock().await;
            if let Err(e) = meta.sync().await {
                error!("Manual sync failed: {}", e);
                return;
            }
        }

        // Trigger re-scan
        info!("Metadata synced, starting full re-scan...");
        let meta_provider = metadata.lock().await;
        let mut new_games = Vec::new();
        for entry in WalkDir::new(&games_dir).into_iter().filter_map(|e| e.ok()) {
            if let Some(game) = process_entry(entry.path(), &games_dir, &data_dir, Some(&meta_provider)) {
                new_games.push(game);
            }
        }
        let mut g_lock = games.lock().unwrap();
        *g_lock = new_games;
        drop(g_lock);

        let _ = tx.send(
            serde_json::json!({
                "type": "scan",
                "status": "complete",
                "count": 0 // Optional: calculate actual count
            })
            .to_string(),
        );
    });

    Json(serde_json::json!({ "status": "started" }))
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
) -> impl IntoResponse {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use tempfile::tempdir;
    use base64::Engine;

    async fn setup_test_app() -> (TestServer, AppState, tempfile::TempDir) {
        let tmp_dir = tempdir().unwrap();
        let games_dir = tmp_dir.path().join("games");
        let data_dir = tmp_dir.path().join("data");
        std::fs::create_dir_all(&games_dir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();

        // Create a dummy game file
        std::fs::write(games_dir.join("Test Game [0100000000010000][v0].nsp"), "dummy").unwrap();

        let settings = Settings {
            server_port: 0,
            games_dir: games_dir.clone(),
            data_dir: data_dir.clone(),
            log_level: "info".to_string(),
            webdav_username: None,
            webdav_password: None,
            webdav_enabled: true,
            metadata_region: "US".to_string(),
            metadata_language: "en".to_string(),
            tinfoil_encrypt: false,
        };

        let games = Arc::new(Mutex::new(vec![Game {
            name: "Test Game".to_string(),
            path: games_dir.join("Test Game [0100000000010000][v0].nsp"),
            relative_path: "Test Game [0100000000010000][v0].nsp".to_string(),
            size: 5,
            format: "nsp".to_string(),
            title_id: Some("0100000000010000".to_string()),
            version: Some("v0".to_string()),
            latest_version: None,
            category: "Base".to_string(),
            publisher: None,
            image_url: None,
        }]));

        let (tx, _) = broadcast::channel(10);
        let metadata = Arc::new(tokio::sync::Mutex::new(
            crate::metadata::MetadataProvider::new(data_dir, "US".to_string(), "en".to_string())
                .await,
        ));
        let dav_handler = webdav::create_dav_handler(&settings);

        let state = AppState {
            games,
            settings,
            host_url: "http://localhost".to_string(),
            downloads: Arc::new(Mutex::new(HashMap::new())),
            tx,
            metadata,
            dav_handler,
        };

        let app = create_app(state.clone());
        (TestServer::new(app).unwrap(), state, tmp_dir)
    }

    #[tokio::test]
    async fn test_list_games() {
        let (server, _, _tmp) = setup_test_app().await;
        let response = server.get("/api/games").await;
        response.assert_status_ok();
        let games: Vec<Game> = response.json();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Test Game");
    }

    #[tokio::test]
    async fn test_tinfoil_index() {
        let (server, _, _tmp) = setup_test_app().await;
        let response = server.get("/tinfoil").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert!(body.get("files").is_some());
        assert!(body.get("success").is_some());
    }

    #[tokio::test]
    async fn test_dbi_index() {
        let (server, _, _tmp) = setup_test_app().await;
        let response = server.get("/dbi").await;
        response.assert_status_ok();
        let body = response.text();
        assert!(body.contains("DBI Index"));
        assert!(body.contains("Test Game"));
    }

    #[tokio::test]
    async fn test_webdav_options() {
        let (server, _, _tmp) = setup_test_app().await;
        
        // With trailing slash
        let response = server.method(axum::http::Method::OPTIONS, "/dav/").await;
        response.assert_status_ok();
        response.assert_header("DAV", "1, 2");
        response.assert_header("MS-Author-Via", "DAV");

        // Without trailing slash
        let response = server.method(axum::http::Method::OPTIONS, "/dav").await;
        response.assert_status_ok();
        response.assert_header("DAV", "1, 2");
        response.assert_header("MS-Author-Via", "DAV");
    }

    #[tokio::test]
    async fn test_webdav_auth() {
        let tmp_dir = tempdir().unwrap();
        let games_dir = tmp_dir.path().join("games");
        let data_dir = tmp_dir.path().join("data");
        std::fs::create_dir_all(&games_dir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();

        let settings = Settings {
            server_port: 0,
            games_dir: games_dir.clone(),
            data_dir: data_dir.clone(),
            log_level: "info".to_string(),
            webdav_username: Some("admin".to_string()),
            webdav_password: Some("password".to_string()),
            webdav_enabled: true,
            metadata_region: "US".to_string(),
            metadata_language: "en".to_string(),
            tinfoil_encrypt: false,
        };

        let games = Arc::new(Mutex::new(vec![]));
        let (tx, _) = broadcast::channel(10);
        let metadata = Arc::new(tokio::sync::Mutex::new(
            crate::metadata::MetadataProvider::new(data_dir, "US".to_string(), "en".to_string())
                .await,
        ));
        let dav_handler = webdav::create_dav_handler(&settings);

        let state = AppState {
            games,
            settings,
            host_url: "http://localhost".to_string(),
            downloads: Arc::new(Mutex::new(HashMap::new())),
            tx,
            metadata,
            dav_handler,
        };

        let app = create_app(state);
        let server = TestServer::new(app).unwrap();

        // 1. Unauthenticated request should fail
        let response = server.get("/dav/").await;
        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
        response.assert_header("WWW-Authenticate", "Basic realm=\"Switcheroo WebDAV\"");

        // 2. Authenticated request should succeed (or at least not be UNAUTHORIZED)
        let auth = base64::engine::general_purpose::STANDARD.encode("admin:password");
        let response = server
            .get("/dav/")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&format!("Basic {}", auth)).unwrap(),
            )
            .await;
        
        // Since it's an empty directory, it might return 200 or 207 depending on the method
        // But it should NOT be 401
        assert_ne!(response.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_spa_fallback() {
        let (server, _, _tmp) = setup_test_app().await;
        
        // Request a random path that doesn't exist
        let response = server.get("/some/random/page").await;
        
        // Should return 200 OK (from index.html)
        response.assert_status_ok();
        response.assert_header("content-type", "text/html");
        
        let body = response.text();
        assert!(body.contains("<div id=\"app\"")); // Basic check for our index.html
    }

    #[tokio::test]
    async fn test_manual_sync_trigger() {
        let (server, _, _tmp) = setup_test_app().await;
        let response = server.get("/api/sync").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("started"));
    }
}
