mod scanner;
mod downloads;
mod config;

use axum::{
    routing::get,
    Router,
    Json,
    extract::{State, Path},
    response::{IntoResponse, sse::{Event, Sse}},
    body::{Body, Bytes},
    http::{header::{HeaderMap, CONTENT_TYPE, CONTENT_DISPOSITION, CONTENT_LENGTH}, HeaderValue},
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::Duration;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use local_ip_address::local_ip;
use scanner::{Game, scan_games, process_entry};
use downloads::{DownloadState, Downloads};
use config::Settings;
use tokio::sync::broadcast;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use futures::stream::{Stream, StreamExt};
use uuid::Uuid;
use tracing::{info, error, debug};
use walkdir::WalkDir;

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
        info!("Games directory not found, creating: {:?}", settings.games_dir);
        std::fs::create_dir_all(&settings.games_dir).expect("Failed to create games directory");
    }

    let games = Arc::new(Mutex::new(Vec::new()));
    
    let local_ip = local_ip().unwrap_or("127.0.0.1".parse().unwrap());
    let host_url = format!("http://{}:{}", local_ip, settings.server_port);

    let downloads: Downloads = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _rx) = broadcast::channel(100);

    let state = AppState {
        games: games.clone(),
        settings: settings.clone(),
        host_url: host_url.clone(),
        downloads: downloads.clone(),
        tx: tx.clone(),
    };

    // Background Game Scanning Task
    let games_dir = settings.games_dir.clone();
    let games_clone = games.clone();
    let tx_scan = tx.clone();

    tokio::task::spawn_blocking(move || {
        info!("Starting background game scan in: {:?}", games_dir);
        let start_time = std::time::Instant::now();
        
        // Notify start
        let _ = tx_scan.send(serde_json::json!({ 
            "type": "scan", 
            "status": "scanning", 
            "count": 0 
        }).to_string());

        let mut batch = Vec::new();
        let mut total_count = 0;
        
        for entry in WalkDir::new(&games_dir).into_iter().filter_map(|e| e.ok()) {
            if let Some(game) = process_entry(&entry, &games_dir) {
                batch.push(game);
                total_count += 1;
                
                // Update batch every 50 items to keep UI responsive but performant
                if batch.len() >= 50 {
                    let mut g_lock = games_clone.lock().unwrap();
                    g_lock.extend(batch.drain(..));
                    drop(g_lock); // Release lock immediately

                    let _ = tx_scan.send(serde_json::json!({ 
                        "type": "scan", 
                        "status": "scanning", 
                        "count": total_count 
                    }).to_string());
                }
            }
        }
        
        // Flush remaining
        if !batch.is_empty() {
            let mut g_lock = games_clone.lock().unwrap();
            g_lock.extend(batch);
        }
        
        let duration = start_time.elapsed();
        info!("Scan complete. Indexed {} games in {:.2?}.", total_count, duration);
        
        let _ = tx_scan.send(serde_json::json!({ 
            "type": "scan", 
            "status": "complete", 
            "count": total_count 
        }).to_string());
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

            if !downloads.is_empty() {
                if let Ok(data_json) = serde_json::to_value(&*downloads) {
                     let msg = serde_json::json!({
                         "type": "downloads",
                         "data": data_json
                     }).to_string();
                     let _ = tx_clone.send(msg);
                }
            }
        }
    });

    let frontend_dist = "frontend/dist";

    let app = Router::new()
        .route("/api/games", get(list_games))
        .route("/api/info", get(server_info))
        .route("/tinfoil", get(tinfoil_index))
        .route("/tinwoo", get(tinfoil_index))
        .route("/dbi", get(dbi_index))
        .route("/events", get(sse_handler))
        .route("/files/{*path}", get(download_file))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
        // Fallback service to handle frontend assets and SPA routing
        .fallback_service(ServeDir::new(frontend_dist).fallback(ServeFile::new(format!("{}/index.html", frontend_dist))));

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

    Json(serde_json::json!({
        "ips": ips,
        "port": state.settings.server_port
    }))
}

async fn list_games(State(state): State<AppState>) -> Json<Vec<Game>> {
    let games = state.games.lock().unwrap();
    Json(games.clone())
}

async fn sse_handler(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .map(|msg| {
             match msg {
                 Ok(msg) => Ok(Event::default().data(msg)),
                 Err(_) => Ok(Event::default().comment("keepalive")),
             }
        });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn download_file(Path(path): Path<String>, State(state): State<AppState>) -> impl IntoResponse {
    let file_path = state.settings.games_dir.join(&path);
    
    if !file_path.starts_with(&state.settings.games_dir) {
        return Err((axum::http::StatusCode::FORBIDDEN, "Forbidden"));
    }

    match File::open(&file_path).await {
        Ok(file) => {
            let metadata = file.metadata().await.unwrap();
            let total_size = metadata.len();
            let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
            
            let download_id = Uuid::new_v4().to_string();
            info!("Starting download: {} (ID: {})", filename, download_id);
            
            {
                let mut downloads = state.downloads.lock().unwrap();
                downloads.insert(download_id.clone(), DownloadState {
                    id: download_id.clone(),
                    filename: filename.clone(),
                    total_size,
                    bytes_sent: 0,
                    speed: 0,
                });
            }
            
            let stream = ReaderStream::new(file);
            let downloads_clone = state.downloads.clone();
            let id_clone = download_id.clone();
            
            let stream = stream.map(move |chunk: Result<Bytes, std::io::Error>| {
                if let Ok(bytes) = &chunk {
                    let len = bytes.len() as u64;
                    if let Ok(mut downloads) = downloads_clone.lock() {
                        if let Some(download) = downloads.get_mut(&id_clone) {
                            download.bytes_sent += len;
                        }
                    }
                }
                chunk
            });
            
            let body = Body::from_stream(stream);
            
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
            if let Ok(val) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)) {
                headers.insert(CONTENT_DISPOSITION, val);
            }
            if let Ok(val) = HeaderValue::from_str(&total_size.to_string()) {
                headers.insert(CONTENT_LENGTH, val);
            }
            
            Ok((headers, body))
        },
        Err(e) => {
            error!("File download failed: {} (Path: {:?})", e, file_path);
            Err((axum::http::StatusCode::NOT_FOUND, "File not found"))
        },
    }
}

async fn tinfoil_index(State(state): State<AppState>) -> Json<serde_json::Value> {
    let games = state.games.lock().unwrap();
    
    let files: Vec<serde_json::Value> = games.iter().map(|game| {
        let url = format!("{}/files/{}", state.host_url, game.relative_path);
        
        serde_json::json!({
            "url": url,
            "size": game.size,
        })
    }).collect();

    Json(serde_json::json!({
        "files": files,
        "success": "The index was generated successfully.",
    }))
}

async fn dbi_index(State(state): State<AppState>) -> axum::response::Html<String> {
    let games = state.games.lock().unwrap();
    
    let mut html = String::from("<!DOCTYPE html><html><head><title>DBI Index</title></head><body><h1>Index of /</h1><ul>");
    
    for game in games.iter() {
        let url = format!("/files/{}", game.relative_path);
        let name = game.name.clone();
        
        html.push_str(&format!("<li><a href=\"{}\">{}</a></li>", url, name));
    }
    
    html.push_str("</ul></body></html>");
    
    axum::response::Html(html)
}