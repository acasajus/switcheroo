use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TitleInfo {
    pub id: String,
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub banner_url: Option<String>,
    pub category: Option<Vec<String>>,
    pub description: Option<String>,
    pub publisher: Option<String>,
}

pub struct MetadataProvider {
    pub data_dir: PathBuf,
    pub region: String,
    pub language: String,
    titles: HashMap<String, TitleInfo>,
    versions: HashMap<String, HashMap<String, String>>, // TitleID -> {Version: Date}
}

impl MetadataProvider {
    pub async fn new(data_dir: PathBuf, region: String, language: String) -> Self {
        Self {
            data_dir,
            region,
            language,
            titles: HashMap::new(),
            versions: HashMap::new(),
        }
    }

    pub async fn init(&mut self) {
        self.load_local_data().await;
    }

    async fn load_local_data(&mut self) {
        let titles_path = self
            .data_dir
            .join("titledb")
            .join(format!("{}.{}.json", self.region, self.language));
        if titles_path.exists() {
            info!("Loading local titles database from {:?}", titles_path);
            let content = tokio::fs::read_to_string(&titles_path).await.unwrap_or_default();
            if !content.is_empty() {
                let titles = tokio::task::spawn_blocking(move || {
                    let mut map = HashMap::new();
                    if let Ok(data) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&content) {
                        for (id, val) in data {
                            let info = TitleInfo {
                                id: id.clone(),
                                name: val.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                icon_url: val.get("iconUrl").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                banner_url: val.get("bannerUrl").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                category: val.get("category").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()),
                                description: val.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                publisher: val.get("publisher").and_then(|v| v.as_str()).map(|s| s.to_string()),
                            };
                            map.insert(id.to_uppercase(), info);
                        }
                    }
                    map
                }).await.unwrap_or_default();
                self.titles = titles;
            }
        }

        let versions_path = self.data_dir.join("titledb").join("versions.json");
        if versions_path.exists() {
            info!("Loading local versions database from {:?}", versions_path);
            if let Ok(content) = tokio::fs::read_to_string(&versions_path).await
                && let Ok(data) =
                    serde_json::from_str::<HashMap<String, HashMap<String, String>>>(&content)
            {
                self.versions = data;
            }
        }
    }

    pub async fn sync(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let titledb_dir = self.data_dir.join("titledb");
        if !titledb_dir.exists() {
            info!("Creating titledb directory: {:?}", titledb_dir);
            tokio::fs::create_dir_all(&titledb_dir).await?;
        }

        let client = reqwest::Client::new();

        // Sync versions.json
        info!("Syncing versions.json...");
        match client.get("https://raw.githubusercontent.com/blawar/titledb/master/versions.json").send().await {
            Ok(resp) if resp.status().is_success() => {
                let mut file = File::create(titledb_dir.join("versions.json")).await?;
                let mut stream = resp.bytes_stream();
                while let Some(item) = stream.next().await {
                    file.write_all(&item?).await?;
                }
            }
            Ok(resp) => warn!("Failed to sync versions.json: status {}", resp.status()),
            Err(e) => warn!("Failed to sync versions.json: {}", e),
        }

        // Try region-specific first, then titles.json
        let filename = format!("{}.{}.json", self.region, self.language);
        let urls = vec![
            format!("https://raw.githubusercontent.com/blawar/titledb/master/{}", filename),
            "https://raw.githubusercontent.com/blawar/titledb/master/titles.json".to_string(),
        ];

        for url in urls {
            info!("Syncing titles from {}...", url);
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let dest = titledb_dir.join(&filename);
                    let mut file = File::create(dest).await?;
                    let mut stream = resp.bytes_stream();
                    while let Some(item) = stream.next().await {
                        file.write_all(&item?).await?;
                    }
                    info!("Successfully synced titles from {}", url);
                    break;
                }
                Ok(resp) => warn!("Failed to sync from {}: status {}", url, resp.status()),
                Err(e) => warn!("Failed to sync from {}: {}", url, e),
            }
        }

        self.load_local_data().await;
        Ok(())
    }

    pub fn get_title_info(&self, title_id: &str) -> Option<&TitleInfo> {
        self.titles.get(&title_id.to_uppercase())
    }

    pub fn get_latest_version(&self, title_id: &str) -> Option<String> {
        let versions = self.versions.get(&title_id.to_lowercase())?;
        versions.keys().filter_map(|v| v.parse::<u64>().ok()).max().map(|v| v.to_string())
    }
}
