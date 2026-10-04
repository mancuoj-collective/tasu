use std::path::PathBuf;

use serde::Deserialize;

/// Zero-config by default: without a `remote`, tasu is purely local and never
/// invokes git. Settings come from the environment first, then a config file.
#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub remote: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let file = FileConfig::read();

        let data_dir = std::env::var_os("TASU_DATA")
            .map(PathBuf::from)
            .or(file.data_dir)
            .unwrap_or_else(default_data_dir);

        let remote = std::env::var("TASU_REMOTE")
            .ok()
            .filter(|url| !url.trim().is_empty())
            .or(file.remote);

        Self { data_dir, remote }
    }

    /// Path of the persisted board file.
    pub fn board_path(&self) -> PathBuf {
        self.data_dir.join("todos.json")
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    data_dir: Option<PathBuf>,
    remote: Option<String>,
}

impl FileConfig {
    /// Config lives beside the code's user config, not in the (synced) data
    /// directory, so it never travels between machines.
    fn read() -> Self {
        let Some(path) = dirs::config_dir().map(|dir| dir.join("tasu").join("config.json")) else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|dir| dir.join("tasu"))
        .unwrap_or_else(|| PathBuf::from("."))
}
