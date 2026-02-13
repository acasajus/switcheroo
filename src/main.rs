mod config;
mod downloads;
mod handlers;
mod metadata;
mod scanner;
mod state;
mod tasks;
mod tinfoil;
mod webdav;

use axum::{
    Router,
    routing::{any, get},
};
use local_ip_address::local_ip;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::{Level, info};

use crate::config::Settings;
use crate::handlers::{api, dbi, files, tinfoil as tinfoil_h, web};
use crate::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let settings = Settings::new().expect("Failed to load configuration");

    tracing_subscriber::fmt()
        .with_env_filter(&settings.log_level)
        .init();

    info!("Starting Switcheroo...");

    if !settings.games_dir.exists() {
        std::fs::create_dir_all(&settings.games_dir).expect("Failed to create games directory");
    }
    if !settings.data_dir.join("images").exists() {
        std::fs::create_dir_all(settings.data_dir.join("images"))
            .expect("Failed to create images directory");
    }

    let games = Arc::new(Mutex::new(Vec::new()));
    let local_ip = local_ip().unwrap_or("127.0.0.1".parse().unwrap());
    let host_url = format!("http://{}:{}", local_ip, settings.server_port);
    let downloads = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _) = broadcast::channel(100);

    let metadata = Arc::new(tokio::sync::Mutex::new(
        crate::metadata::MetadataProvider::new(
            settings.data_dir.clone(),
            settings.metadata_region.clone(),
            settings.metadata_language.clone(),
        )
        .await,
    ));

    let dav_handler = webdav::create_dav_handler(&settings);

    let state = AppState {
        games,
        settings: settings.clone(),
        host_url: host_url.clone(),
        downloads,
        tx,
        metadata: metadata.clone(),
        dav_handler,
    };

    // Metadata Init
    let metadata_init = metadata.clone();
    tokio::spawn(async move {
        let mut meta = metadata_init.lock().await;
        meta.init().await;
        info!("Metadata initialized and ready.");
    });

    // Start background tasks (Scanning, Speed, Watcher, Sync)
    tasks::start_background_tasks(state.clone());

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
    let main_routes = Router::new()
        .route("/api/games", get(api::list_games))
        .route("/api/info", get(api::server_info))
        .route("/api/sync", get(api::sync_metadata))
        .route("/tinfoil", get(tinfoil_h::tinfoil_index))
        .route("/tinfoil/", get(tinfoil_h::tinfoil_index))
        .route("/tinwoo", get(tinfoil_h::tinfoil_index))
        .route("/tinwoo/", get(tinfoil_h::tinfoil_index))
        .route("/dbi", get(dbi::dbi_index))
        .route("/dbi/", get(dbi::dbi_index))
        .route("/dbi/{*path}", get(files::download_file))
        .route("/events", get(api::sse_handler))
        .route("/files/{*path}", get(files::download_file))
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

    let mut app = Router::new().merge(main_routes);

    if state.settings.webdav_enabled {
        app = app
            .route("/dav", any(webdav_wrapper))
            .route("/dav/", any(webdav_wrapper))
            .route("/dav/{*path}", any(webdav_wrapper));
    }

    app.with_state(state).fallback(web::static_handler)
}

async fn webdav_wrapper(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
) -> impl axum::response::IntoResponse {
    use axum::http::header::HeaderValue;
    use axum::response::IntoResponse;

    let method = req.method().clone();
    let uri = req.uri().clone();

    let mut response =
        webdav::webdav_handler(state.settings.clone(), state.dav_handler.clone(), req)
            .await
            .into_response();

    let headers = response.headers_mut();
    headers.insert("DAV", HeaderValue::from_static("1, 2"));
    headers.insert("MS-Author-Via", HeaderValue::from_static("DAV"));

    if !response.status().is_success() {
        tracing::debug!(
            "WebDAV Response Error: {} {} -> {}",
            method,
            uri,
            response.status()
        );
    } else {
        info!(
            "WebDAV Request: {} {} -> {}",
            method,
            uri,
            response.status()
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::Game;
    use axum_test::TestServer;
    use base64::Engine;
    use tempfile::tempdir;

    async fn setup_test_app() -> (TestServer, AppState, tempfile::TempDir) {
        let tmp_dir = tempdir().unwrap();
        let games_dir = tmp_dir.path().join("games");
        let data_dir = tmp_dir.path().join("data");
        std::fs::create_dir_all(&games_dir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();

        std::fs::write(
            games_dir.join("Test Game [0100000000010000][v0].nsp"),
            "dummy",
        )
        .unwrap();

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
        let response = server.method(axum::http::Method::OPTIONS, "/dav/").await;
        response.assert_status_ok();
        response.assert_header("DAV", "1, 2");
        response.assert_header("MS-Author-Via", "DAV");

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

        let response = server.get("/dav/").await;
        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
        response.assert_header("WWW-Authenticate", "Basic realm=\"Switcheroo WebDAV\"");

        let auth = base64::engine::general_purpose::STANDARD.encode("admin:password");
        let response = server
            .get("/dav/")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&format!("Basic {}", auth)).unwrap(),
            )
            .await;

        assert_ne!(response.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_spa_fallback() {
        let (server, _, _tmp) = setup_test_app().await;
        let response = server.get("/some/random/page").await;
        response.assert_status_ok();
        response.assert_header("content-type", "text/html");
        let body = response.text();
        assert!(body.contains("<div id=\"app\""));
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
