use std::ffi::OsString;
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
        Self::resolve(
            std::env::var_os("TASU_DATA"),
            std::env::var_os("TASU_REMOTE"),
            FileConfig::read(),
        )
    }

    /// Apply the precedence — environment over config file over default — from
    /// already-read values, so it can be tested without touching the process
    /// environment. A *present* environment value always wins, even when empty:
    /// an empty `TASU_DATA`/`TASU_REMOTE` means "not set here", never "fall back
    /// to the file".
    fn resolve(data: Option<OsString>, remote: Option<OsString>, file: FileConfig) -> Self {
        let data_dir = data
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or(file.data_dir)
            .unwrap_or_else(default_data_dir);

        let remote = match remote {
            None => file.remote,
            Some(value) => match value.into_string() {
                Ok(url) if url.trim().is_empty() => None,
                Ok(url) => Some(url),
                Err(_) => {
                    eprintln!("tasu: ignoring non-UTF-8 TASU_REMOTE");
                    file.remote
                }
            },
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    fn file(data: Option<&str>, remote: Option<&str>) -> FileConfig {
        FileConfig {
            data_dir: data.map(PathBuf::from),
            remote: remote.map(str::to_string),
        }
    }

    #[test]
    fn empty_env_data_dir_does_not_become_the_cwd() {
        let config = Config::resolve(
            Some(OsString::from("")),
            None,
            file(Some("/from/file"), None),
        );
        assert_eq!(config.data_dir, PathBuf::from("/from/file"));
    }

    #[test]
    fn empty_env_remote_disables_a_file_remote() {
        let config = Config::resolve(
            None,
            Some(OsString::from("")),
            file(None, Some("git@host:repo.git")),
        );
        assert_eq!(
            config.remote, None,
            "a present-but-empty TASU_REMOTE must override the config file"
        );
    }

    #[test]
    fn a_set_env_remote_wins_over_the_file() {
        let config = Config::resolve(
            None,
            Some(OsString::from("git@env:repo.git")),
            file(None, Some("git@file:repo.git")),
        );
        assert_eq!(config.remote.as_deref(), Some("git@env:repo.git"));
    }

    #[test]
    fn absent_env_remote_uses_the_file() {
        let config = Config::resolve(None, None, file(None, Some("git@file:repo.git")));
        assert_eq!(config.remote.as_deref(), Some("git@file:repo.git"));
    }
}
