use futures::StreamExt;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

const IMAGE_SOURCES: &[&str] = &[
    "https://raw.githubusercontent.com/mxm-madscience/switch-games/master/images/{id}.jpg",
    "https://raw.githubusercontent.com/TheGameratorT/Switch-Icons/main/icons/{id}.jpg",
    "https://raw.githubusercontent.com/sblantipodi/switch_icon_db/main/icons/{id}.jpg",
    "https://tinfoil.media/title/{id}/0",
];

fn get_base_id(title_id: &str) -> Option<String> {
    if let Ok(id) = u64::from_str_radix(title_id, 16) {
        // Simple heuristic: if it's an update, the base ID is usually the same with the last 3 digits cleared (roughly)
        // or specifically masking out the type.
        // Base Game: ending in 000, 200, 400, 600, 800, A00, C00, E00?
        // Actually, Updates usually add 0x800 to the base?
        // Let's try the standard mask for Application ID vs AddOn/Update.
        // ApplicationId = TitleId & 0xFFFFFFFFFFFFE000
        let base_id = id & 0xFFFFFFFFFFFFE000;
        let base_id_str = format!("{:016X}", base_id);

        if base_id_str != title_id.to_uppercase() {
            return Some(base_id_str);
        }
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

            info!("Trying to fetch image from: {}", url);

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
                "jpg"
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
            info!(
                "Downloaded image for {} (using ID: {}) to {:?}",
                title_id, id, final_path
            );
            return Some(final_path);
        }
    }

    warn!("Could not find image for {}", title_id);
    None
}
