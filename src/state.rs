use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use crate::config::Settings;
use crate::scanner::Game;
use crate::downloads::Downloads;
use crate::metadata::MetadataProvider;
use dav_server::DavHandler;

#[derive(Clone)]
pub struct AppState {
    pub games: Arc<Mutex<Vec<Game>>>,
    pub settings: Settings,
    pub host_url: String,
    pub downloads: Downloads,
    pub tx: broadcast::Sender<String>,
    pub metadata: Arc<tokio::sync::Mutex<MetadataProvider>>,
    pub dav_handler: DavHandler,
}
