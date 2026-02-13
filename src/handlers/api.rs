use axum::{
    Json,
    extract::State,
    response::sse::{Event, Sse},
};
use futures::stream::{Stream, StreamExt};
use tracing::{info, error};
use walkdir::WalkDir;
use crate::state::AppState;
use crate::scanner::process_entry;

pub async fn server_info(State(state): State<AppState>) -> Json<serde_json::Value> {
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

pub async fn list_games(State(state): State<AppState>) -> Json<Vec<crate::scanner::Game>> {
    let games = state.games.lock().unwrap();
    Json(games.clone())
}

pub async fn sync_metadata(State(state): State<AppState>) -> Json<serde_json::Value> {
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
                "count": 0
            })
            .to_string(),
        );
    });

    Json(serde_json::json!({ "status": "started" }))
}

pub async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(|msg| match msg {
        Ok(msg) => Ok(Event::default().data(msg)),
        Err(_) => Ok(Event::default().comment("keepalive")),
    });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
