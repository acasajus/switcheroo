use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::fmt;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub server_port: u16,
    pub games_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_level: String,
    pub webdav_username: Option<String>,
    pub webdav_password: Option<String>,
    pub webdav_enabled: bool,
    pub metadata_region: String,
    pub metadata_language: String,
    pub tinfoil_encrypt: bool,
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("server_port", &self.server_port)
            .field("games_dir", &self.games_dir)
            .field("data_dir", &self.data_dir)
            .field("log_level", &self.log_level)
            .field("webdav_enabled", &self.webdav_enabled)
            .field("metadata_region", &self.metadata_region)
            .field("metadata_language", &self.metadata_language)
            .field(
                "webdav_username",
                &self.webdav_username.as_ref().map(|_| "***"),
            )
            .field(
                "webdav_password",
                &self.webdav_password.as_ref().map(|_| "***"),
            )
            .finish()
    }
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            // Default settings
            .set_default("server_port", 3000)?
            .set_default("games_dir", "./games")?
            .set_default("data_dir", "./data")?
            .set_default("log_level", "info")?
            .set_default("webdav_username", None::<String>)?
            .set_default("webdav_password", None::<String>)?
            .set_default("webdav_enabled", true)?
            .set_default("metadata_region", "US")?
            .set_default("metadata_language", "en")?
            .set_default("tinfoil_encrypt", false)?
            // Config file (optional)
            .add_source(File::with_name("config").required(false))
            // Environment variables (e.g. SWITCHEROO_SERVER_PORT=8080)
            .add_source(Environment::with_prefix("SWITCHEROO"));

        builder.build()?.try_deserialize()
    }
}
