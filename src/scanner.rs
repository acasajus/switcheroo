use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Debug, Deserialize)]
pub struct Game {
    pub name: String,
    pub path: PathBuf,
    pub relative_path: String,
    pub size: u64,
    pub format: String,
    pub title_id: Option<String>,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub category: String, // "Base", "Update", "DLC"
    pub publisher: Option<String>,
    pub image_url: Option<String>,
}

fn parse_filename(filename: &str) -> (String, Option<String>, Option<String>, String) {
    // defaults
    let mut clean_name = filename.to_string();
    let mut title_id = None;
    let mut version = None;
    let mut category = "Base".to_string();

    // specific extension removal
    if let Some(stem) = Path::new(filename).file_stem().and_then(|s| s.to_str()) {
        clean_name = stem.to_string();
    }

    // Attempt to parse standard format: Name [ID][vVersion]...
    // Very basic parser: look for bracketed sections
    let mut name_parts = Vec::new();
    let parts: Vec<&str> = clean_name.split('[').collect();

    if parts.is_empty() {
        return (clean_name, title_id, version, category);
    }

    // The first part is usually the name (trimmed)
    name_parts.push(parts[0].trim());

    for part in &parts[1..] {
        let end = match part.find(']') {
            Some(e) => e,
            None => continue,
        };
        let content = &part[..end];

        // Heuristics
        if content.len() == 16 && content.chars().all(|c| c.is_ascii_hexdigit()) {
            title_id = Some(content.to_uppercase());
            continue;
        }

        if content.starts_with('v') && content[1..].chars().all(|c| c.is_ascii_digit()) {
            version = Some(content.to_string());
            // If version is not v0, it might be an update, but usually the Title ID tells us more.
            // For simplicity, if version > 0, we can flag it, or wait for explicit [UPD] tags.
            // Actually, let's treat everything as Base unless we see update markers or different ID logic
            if content != "v0" {
                category = "Update".to_string();
            }
            continue;
        }

        if content == "UPD" {
            category = "Update".to_string();
            continue;
        }

        if content == "DLC" {
            category = "DLC".to_string();
            continue;
        }
    }

    let final_name = if name_parts.is_empty() {
        clean_name
    } else {
        name_parts.join(" ")
    };

    // Fallback: if we have an ID and it ends in 800/8000 (usually updates) or something?
    // Simplified logic for now.

    (final_name, title_id, version, category)
}

pub fn process_entry(
    path: &Path,
    root_dir: &Path,
    data_dir: &Path,
    metadata: Option<&crate::metadata::MetadataProvider>,
) -> Option<Game> {
    let valid_extensions = ["nsp", "nsz", "xci", "xcz"];

    if !path.is_file() {
        return None;
    }

    let ext = path.extension()?.to_str()?;
    if !valid_extensions.contains(&ext.to_lowercase().as_str()) {
        return None;
    }

    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let relative_path = path
        .strip_prefix(root_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let (mut name, title_id, version, category) = parse_filename(&filename);
    let mut publisher = None;
    let mut latest_version = None;

    // Enhance info from metadata provider if available
    if let (Some(provider), Some(tid)) = (metadata, title_id.as_ref()) {
        if let Some(info) = provider.get_title_info(tid) {
            if let Some(ref n) = info.name {
                name = n.clone();
            }
            publisher = info.publisher.clone();
        }
        latest_version = provider.get_latest_version(tid);
    }

    // Check for image
    let mut image_url = None;
    let image_exts = ["jpg", "png", "jpeg", "webp"];

    // 1. Check data_dir/images cache
    if let Some(ref tid) = title_id {
        for img_ext in image_exts {
            let img_path = data_dir.join("images").join(format!("{}.{}", tid, img_ext));
            if img_path.exists() {
                image_url = Some(format!("/images/{}.{}", tid, img_ext));
                break;
            }
        }
    }

    // 2. Check local file (fallback)
    if image_url.is_none() {
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        for img_ext in image_exts {
            let img_path = path.with_file_name(format!("{}.{}", file_stem, img_ext));
            if img_path.exists() {
                let rel_img = img_path
                    .strip_prefix(root_dir)
                    .unwrap_or(&img_path)
                    .to_string_lossy()
                    .to_string();
                image_url = Some(rel_img);
                break;
            }
        }
    }

    Some(Game {
        name,
        path: path.to_path_buf(),
        relative_path,
        size,
        format: ext.to_lowercase(),
        title_id,
        version,
        latest_version,
        category,
        publisher,
        image_url,
    })
}
