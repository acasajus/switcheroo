use walkdir::{WalkDir, DirEntry};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Debug, Deserialize)]
pub struct Game {
    pub name: String,
    pub path: PathBuf,
    pub relative_path: String,
    pub size: u64,
    pub format: String,
}

pub fn process_entry(entry: &DirEntry, root_dir: &Path) -> Option<Game> {
    let path = entry.path();
    let valid_extensions = vec!["nsp", "nsz", "xci", "xcz"];

    if path.is_file() {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if valid_extensions.contains(&ext.to_lowercase().as_str()) {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("Unknown").to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let relative_path = path.strip_prefix(root_dir).unwrap_or(path).to_string_lossy().to_string();
                
                return Some(Game {
                    name,
                    path: path.to_path_buf(),
                    relative_path,
                    size,
                    format: ext.to_lowercase(),
                });
            }
        }
    }
    None
}

pub fn scan_games(root_dir: &Path) -> Vec<Game> {
    WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| process_entry(&e, root_dir))
        .collect()
}

