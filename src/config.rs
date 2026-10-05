use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

        let remote = match std::env::var("TASU_REMOTE") {
            Ok(url) if !url.trim().is_empty() => Some(url),
            Ok(_) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                eprintln!("tasu: ignoring non-UTF-8 TASU_REMOTE");
                None
            }
            Err(std::env::VarError::NotPresent) => None,
        }
        .or(file.remote);

        Self { data_dir, remote }
    }

    /// Path of the persisted board file.
    pub fn board_path(&self) -> PathBuf {
        self.data_dir.join("todos.json")
    }

    /// Path of the config file. Kept in an XDG-style directory, **not** the
    /// platform config dir: on macOS and Windows that equals the data dir, so
    /// the config would land inside the synced Git working tree, where a
    /// checkout can overwrite it with whatever the remote has.
    pub fn config_file() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?;
        Some(base.join("tasu").join("config.json"))
    }

    /// Set (or clear) the sync remote in the config file.
    pub fn set_remote(remote: Option<&str>) -> std::io::Result<PathBuf> {
        let mut file = FileConfig::read();
        file.remote = remote.map(str::to_string);
        file.write()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct FileConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
}

impl FileConfig {
    fn read() -> Self {
        let Some(path) = Config::config_file() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text, &path),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                eprintln!("tasu: could not read {}: {err}", path.display());
                Self::default()
            }
        }
    }

    fn parse(text: &str, path: &Path) -> Self {
        match serde_json::from_str(text) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("tasu: ignoring malformed {}: {err}", path.display());
                Self::default()
            }
        }
    }

    fn write(&self) -> std::io::Result<PathBuf> {
        let path = Config::config_file()
            .ok_or_else(|| std::io::Error::other("no config directory on this platform"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|dir| dir.join("tasu"))
        .unwrap_or_else(|| PathBuf::from("."))
}
