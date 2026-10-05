use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::{Board, Task};

/// On-disk schema version. Bump when the shape changes; migration lands here.
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct FileSchema {
    version: u32,
    #[serde(default)]
    tasks: Vec<Task>,
}

/// Single-file JSON persistence. Loading never fails and never destroys data:
/// an unreadable file is backed up beside itself and an empty board is used.
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the board. Missing file yields an empty board; a corrupt file is
    /// moved aside (`.corrupt-<unix-seconds>`) so it can be inspected later.
    pub fn load(&self) -> Board {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Board::new();
        };
        match serde_json::from_str::<FileSchema>(&text) {
            Ok(schema) => Board::from_tasks(schema.tasks),
            Err(_) => {
                self.backup();
                Board::new()
            }
        }
    }

    /// Atomically replace the file (temp file + rename) so a crash mid-write
    /// cannot leave a half-written board.
    pub fn save(&self, board: &Board) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("failed to create data directory")?;
        }
        let schema = FileSchema {
            version: SCHEMA_VERSION,
            tasks: board.tasks().to_vec(),
        };
        let json = serde_json::to_string_pretty(&schema).context("failed to serialize board")?;
        atomic_write(&self.path, json.as_bytes())
    }

    /// Last modification time, for detecting external writes.
    pub fn mtime(&self) -> Option<SystemTime> {
        fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok()
    }

    fn backup(&self) {
        let file_name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "todos.json".to_string());
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let backup = self
            .path
            .with_file_name(format!("{file_name}.corrupt-{seconds}"));
        if let Err(err) = fs::rename(&self.path, &backup) {
            eprintln!("tasu: could not back up {}: {err}", self.path.display());
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "todos.json".to_string())
    ));
    fs::write(&tmp, bytes).context("failed to write temporary file")?;
    if fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    // Windows cannot rename over an existing file.
    let _ = fs::remove_file(path);
    fs::rename(&tmp, path).context("failed to replace board file")
}

/// Union two serialized boards, de-duplicating identical tasks. Returns `None`
/// when either side is unreadable or from another schema version, so the caller
/// can fall back to one side rather than guess.
pub fn merge_files(local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    let local: FileSchema = serde_json::from_slice(local).ok()?;
    let remote: FileSchema = serde_json::from_slice(remote).ok()?;
    if local.version != SCHEMA_VERSION || remote.version != SCHEMA_VERSION {
        return None;
    }
    let merged = Board::from_tasks(local.tasks).merged_with(&Board::from_tasks(remote.tasks));
    let schema = FileSchema {
        version: SCHEMA_VERSION,
        tasks: merged.into_tasks(),
    };
    serde_json::to_vec_pretty(&schema).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_time::at;

    fn board_bytes(titles: &[&str]) -> Vec<u8> {
        let schema = FileSchema {
            version: SCHEMA_VERSION,
            tasks: titles
                .iter()
                .map(|t| Task::new(*t, at(2026, 10, 5)))
                .collect(),
        };
        serde_json::to_vec(&schema).unwrap()
    }

    #[test]
    fn merging_files_unions_and_dedupes() {
        let local = board_bytes(&["shared", "local only"]);
        let remote = board_bytes(&["shared", "remote only"]);
        let merged: FileSchema =
            serde_json::from_slice(&merge_files(&local, &remote).unwrap()).unwrap();
        let titles: Vec<&str> = merged.tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["shared", "local only", "remote only"]);
    }

    #[test]
    fn merging_refuses_input_it_cannot_safely_merge() {
        let ok = board_bytes(&["a"]);
        assert!(merge_files(b"not json", &ok).is_none());
        assert!(merge_files(&ok, b"{\"version\":99,\"tasks\":[]}").is_none());
    }

    fn store_in(dir: &tempfile::TempDir) -> Store {
        Store::new(dir.path().join("todos.json"))
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let board = store_in(&dir).load();
        assert!(board.is_empty());
    }

    #[test]
    fn roundtrips_a_board() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        let mut board = Board::new();
        board.add("Nand2Tetris 第六章", at(2026, 10, 5));
        board.add("学习 GPUI", at(2026, 10, 6));

        store.save(&board).unwrap();
        let loaded = store.load();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.task(0).unwrap().title, "Nand2Tetris 第六章");
        assert_eq!(loaded.task(1).unwrap().title, "学习 GPUI");
    }

    #[test]
    fn writes_a_versioned_schema() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.save(&Board::new()).unwrap();

        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(text.contains("\"version\""));
        assert!(
            serde_json::from_str::<serde_json::Value>(&text)
                .unwrap()
                .get("version")
                .is_some()
        );
    }

    #[test]
    fn corrupt_file_is_backed_up_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(store.path(), "{ not json").unwrap();

        let board = store.load();
        assert!(board.is_empty());

        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn save_replaces_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        let mut board = Board::new();
        board.add("old", at(2026, 10, 5));
        store.save(&board).unwrap();

        let mut board = Board::new();
        board.add("new", at(2026, 10, 6));
        store.save(&board).unwrap();

        assert_eq!(store.load().len(), 1);
        assert_eq!(store.load().task(0).unwrap().title, "new");
    }

    #[test]
    fn mtime_is_available_once_saved() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        assert!(store.mtime().is_none());
        store.save(&Board::new()).unwrap();
        assert!(store.mtime().is_some());
    }
}
