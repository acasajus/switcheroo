use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[derive(Clone, Debug, serde::Serialize)]
pub struct DownloadState {
    pub id: String,
    pub filename: String,
    pub total_size: u64,
    pub bytes_sent: u64,
    pub speed: u64, // bytes per second
}

pub type Downloads = Arc<Mutex<HashMap<String, DownloadState>>>;
