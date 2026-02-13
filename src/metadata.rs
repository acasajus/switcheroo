use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

const IMAGE_SOURCES: &[&str] = &[
    "https://api.nlib.cc/nx/{id}/icon",
    "https://raw.githubusercontent.com/BigOnYa/titledb/main/icons/{id}.png",
    "https://raw.githubusercontent.com/CensoredTheInvisable/titledb/main/icons/{id}.png",
];

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
                    let dest = if url.contains("titles.json") {
                        titledb_dir.join(&filename) // save as region-specific anyway
                    } else {
                        titledb_dir.join(&filename)
                    };
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

fn get_base_id(title_id: &str) -> Option<String> {
    let id = u64::from_str_radix(title_id, 16).ok()?;
    // ApplicationId = TitleId & 0xFFFFFFFFFFFFE000
    let base_id = id & 0xFFFFFFFFFFFFE000;
    let base_id_str = format!("{:016X}", base_id);

    if base_id_str != title_id.to_uppercase() {
        return Some(base_id_str);
    }
    None
}

pub async fn download_image(title_id: &str, target_path: PathBuf) -> Option<PathBuf> {
    let client = reqwest::Client::new();

    // IDs to try: the provided one, and potentially the base one if it looks like an update/DLC
    let mut ids_to_try = vec![title_id.to_string()];
    if let Some(base) = get_base_id(title_id) {
        ids_to_try.push(base);
    }

    for id in ids_to_try {
        for source in IMAGE_SOURCES {
            let url = source.replace("{id}", &id);

            debug!("Trying to fetch image from: {}", url);

            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    warn!("Request failed for {}: {}", url, e);
                    continue;
                }
            };

            if !resp.status().is_success() {
                debug!("Source {} returned status {}", url, resp.status());
                continue;
            }

            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("");

            // Determine extension from content-type or url
            let ext = if content_type.contains("png") {
                "png"
            } else if content_type.contains("jpeg") || content_type.contains("jpg") {
                "jpg"
            } else {
                // Default fallback
                "png"
            };

            let final_path = target_path.with_extension(ext);

            // Write to file
            let mut file = match File::create(&final_path).await {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to create image file: {}", e);
                    continue;
                }
            };

            let mut stream = resp.bytes_stream();
            while let Some(item) = stream.next().await {
                let chunk = match item {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Failed to read image stream: {}", e);
                        return None;
                    }
                };

                if let Err(e) = file.write_all(&chunk).await {
                    error!("Failed to write image data: {}", e);
                    return None;
                }
            }
            debug!(
                "Downloaded image for {} (using ID: {}) to {:?}",
                title_id, id, final_path
            );
            return Some(final_path);
        }
    }

    warn!("Could not find image for {}", title_id);
    None
}
