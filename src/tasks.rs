use crate::scanner::process_entry;
use crate::state::AppState;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{error, info};
use walkdir::WalkDir;

pub fn start_background_tasks(state: AppState) {
    // 1. Metadata Sync Task
    let state_sync = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(24 * 3600)); // Every 24h
        loop {
            interval.tick().await;
            info!("Starting periodic metadata sync...");
            let mut meta = state_sync.metadata.lock().await;
            if let Err(e) = meta.sync().await {
                error!("Failed to sync metadata: {}", e);
            } else {
                info!("Metadata sync complete.");
                let _ = state_sync.tx.send(
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

    // 2. Download Speed Calculator Task
    let state_speed = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_bytes_map: HashMap<String, u64> = HashMap::new();

        loop {
            interval.tick().await;
            let mut downloads = state_speed.downloads.lock().unwrap();
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

            if !downloads.is_empty()
                && let Ok(data_json) = serde_json::to_value(&*downloads)
            {
                let msg = serde_json::json!({
                    "type": "downloads",
                    "data": data_json
                })
                .to_string();
                let _ = state_speed.tx.send(msg);
            }
        }
    });

    // 3. Initial Game Scanning Task
    let state_scan = state.clone();
    tokio::task::spawn_blocking(move || {
        info!(
            "Starting background game scan in: {:?}",
            state_scan.settings.games_dir
        );
        let start_time = std::time::Instant::now();

        let _ = state_scan.tx.send(
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
        let meta_provider_guard = handle.block_on(state_scan.metadata.lock());

        for entry in WalkDir::new(&state_scan.settings.games_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if let Some(game) = process_entry(
                entry.path(),
                &state_scan.settings.games_dir,
                &state_scan.settings.data_dir,
                Some(&meta_provider_guard),
            ) {
                batch.push(game);
                total_count += 1;

                if batch.len() >= 50 {
                    let mut g_lock = state_scan.games.lock().unwrap();
                    g_lock.extend(batch.drain(..));
                    drop(g_lock);

                    let _ = state_scan.tx.send(
                        serde_json::json!({
                            "type": "scan",
                            "status": "scanning",
                            "count": total_count
                        })
                        .to_string(),
                    );
                }
            }
        }

        if !batch.is_empty() {
            let mut g_lock = state_scan.games.lock().unwrap();
            g_lock.extend(batch);
        }

        info!(
            "Scan complete. Indexed {} games in {:.2?}.",
            total_count,
            start_time.elapsed()
        );

        let _ = state_scan.tx.send(
            serde_json::json!({
                "type": "scan",
                "status": "complete",
                "count": total_count
            })
            .to_string(),
        );
    });

    // 4. File Watcher Task
    let state_watch = state.clone();
    tokio::task::spawn_blocking(move || {
        let (std_tx, std_rx) = channel();
        let mut watcher =
            RecommendedWatcher::new(std_tx, Config::default()).expect("Failed to create watcher");

        watcher
            .watch(&state_watch.settings.games_dir, RecursiveMode::Recursive)
            .expect("Failed to watch games directory");
        info!(
            "File watcher started for: {:?}",
            state_watch.settings.games_dir
        );

        for event in std_rx.into_iter().flatten() {
            use notify::EventKind;
            use notify::event::{ModifyKind, RenameMode};

            match event.kind {
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                    if event.paths.len() == 2 {
                        let from = &event.paths[0];
                        let to = &event.paths[1];

                        let mut games = state_watch.games.lock().unwrap();
                        if let Some(idx) = games.iter().position(|g| g.path == *from) {
                            games.remove(idx);
                            let _ = state_watch.tx.send(
                                serde_json::json!({ "type": "scan", "status": "remove", "path": from })
                                    .to_string(),
                            );
                        }
                        drop(games);

                        let handle = tokio::runtime::Handle::current();
                        let meta_provider = handle.block_on(state_watch.metadata.lock());
                        if let Some(game) = process_entry(
                            to,
                            &state_watch.settings.games_dir,
                            &state_watch.settings.data_dir,
                            Some(&meta_provider),
                        ) {
                            let mut games = state_watch.games.lock().unwrap();
                            games.push(game.clone());
                            let _ = state_watch.tx.send(
                                serde_json::json!({ "type": "scan", "status": "update", "game": game })
                                    .to_string(),
                            );
                        }
                    }
                }
                EventKind::Create(_) | EventKind::Modify(_) => {
                    for path in event.paths {
                        if path.is_file() {
                            let handle = tokio::runtime::Handle::current();
                            let meta_provider = handle.block_on(state_watch.metadata.lock());
                            if let Some(game) = process_entry(
                                &path,
                                &state_watch.settings.games_dir,
                                &state_watch.settings.data_dir,
                                Some(&meta_provider),
                            ) {
                                let mut games = state_watch.games.lock().unwrap();
                                if let Some(idx) = games.iter().position(|g| g.path == game.path) {
                                    games[idx] = game.clone();
                                } else {
                                    games.push(game.clone());
                                }
                                let _ = state_watch.tx.send(
                                    serde_json::json!({ "type": "scan", "status": "update", "game": game })
                                        .to_string(),
                                );
                            }
                        }
                    }
                }
                EventKind::Remove(_) => {
                    for path in event.paths {
                        let mut games = state_watch.games.lock().unwrap();
                        if let Some(idx) = games.iter().position(|g| g.path == path) {
                            games.remove(idx);
                            let _ = state_watch.tx.send(
                                serde_json::json!({ "type": "scan", "status": "remove", "path": path })
                                    .to_string(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    });
}
