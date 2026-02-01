use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server_port: u16,
    pub games_dir: PathBuf,
    pub log_level: String,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            // Default settings
            .set_default("server_port", 3000)?
            .set_default("games_dir", "./games")?
            .set_default("log_level", "info")?
            // Config file (optional)
            .add_source(File::with_name("config").required(false))
            // Environment variables (e.g. SWITCHEROO_SERVER_PORT=8080)
            .add_source(Environment::with_prefix("SWITCHEROO"));

        builder.build()?.try_deserialize()
    }
}
