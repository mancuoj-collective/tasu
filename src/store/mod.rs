use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::{Board, Task};

#[derive(Debug, Serialize, Deserialize)]
struct FileSchema {
    #[serde(default)]
    tasks: Vec<Task>,
}

/// The outcome of reading the board file. `damaged` is set when the file existed
/// but could not be read, so every caller can tell the user instead of quietly
/// continuing with an empty board.
pub struct Loaded {
    pub board: Board,
    pub damaged: Option<Damage>,
}

/// A board file that could not be read and was moved aside, so the failure can
/// be surfaced rather than swallowed.
#[derive(Debug, Clone)]
pub struct Damage {
    /// Where the unreadable file was moved, when the rename succeeded.
    pub backup: Option<PathBuf>,
    /// Why the read failed: a JSON parse error, or an I/O error.
    pub reason: String,
}

impl Damage {
    /// A fuller line for stderr: why the read failed and where the original went.
    pub fn detail(&self) -> String {
        match &self.backup {
            Some(path) => format!(
                "board unreadable ({}) \u{b7} saved as {}",
                self.reason,
                path.display()
            ),
            None => format!("board unreadable ({})", self.reason),
        }
    }
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

    /// Load the board. A missing file is the normal empty board; a file that
    /// cannot be read is moved aside (`.corrupt-<unix-seconds>`) and reported in
    /// [`Loaded::damaged`] so the caller can tell the user.
    pub fn load(&self) -> Loaded {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            // A missing file is the ordinary first-run state, not damage.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Loaded {
                    board: Board::new(),
                    damaged: None,
                };
            }
            // Any other read failure (permissions, a directory in the way) leaves
            // the board just as unreadable, so it is reported too.
            Err(err) => {
                return Loaded {
                    board: Board::new(),
                    damaged: Some(Damage {
                        backup: None,
                        reason: err.to_string(),
                    }),
                };
            }
        };
        match serde_json::from_str::<FileSchema>(&text) {
            Ok(schema) => Loaded {
                board: Board::from_tasks(schema.tasks),
                damaged: None,
            },
            Err(err) => {
                let reason = err.to_string();
                let (backup, reason) = match self.backup() {
                    Ok(path) => (Some(path), reason),
                    Err(move_err) => (
                        None,
                        format!("{reason}; could not move it aside: {move_err}"),
                    ),
                };
                Loaded {
                    board: Board::new(),
                    damaged: Some(Damage { backup, reason }),
                }
            }
        }
    }

    /// Atomically replace the file (temp file + rename) so a crash mid-write
    /// cannot leave a half-written board.
    pub fn save(&self, board: &Board) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("failed to create data directory")?;
        }
        let json = serialize(board).context("failed to serialize board")?;
        atomic_write(&self.path, &json)
    }

    /// Read the raw board bytes: `Ok(None)` when the file is missing, `Err` when
    /// it exists but cannot be read. The sync layer merges serialized boards and
    /// needs the bytes as-is, and must tell "no local board" apart from "the
    /// local board could not be read" so a read failure never looks like an
    /// absent side of the merge.
    pub fn read_bytes(&self) -> std::io::Result<Option<Vec<u8>>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Atomically replace the board with raw bytes, through the same temp-file
    /// path [`Store::save`] uses. Sync must go through here: a second, plain
    /// `fs::write` on the same file would race the reader.
    pub fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("failed to create data directory")?;
        }
        atomic_write(&self.path, bytes)
    }

    /// Last modification time, for detecting external writes.
    pub fn mtime(&self) -> Option<SystemTime> {
        fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok()
    }

    /// Move a corrupt file aside, so it is kept rather than destroyed. When the
    /// rename fails the file stays in place and the error is returned.
    fn backup(&self) -> std::io::Result<PathBuf> {
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
        fs::rename(&self.path, &backup)?;
        Ok(backup)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    // A unique temp name: two writers sharing one `todos.json.tmp` could rename
    // each other's half-written file into place. The counter alone would do, but
    // the pid keeps separate processes clear too.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_file_name(format!(
        "{}.{}.{seq}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "todos.json".to_string()),
        std::process::id(),
    ));
    fs::write(&tmp, bytes).context("failed to write temporary file")?;
    // Never delete the destination first: a failed rename must leave the previous
    // board intact, not open a window with no `todos.json` at all. `fs::rename`
    // already replaces an existing file on Windows (`MOVEFILE_REPLACE_EXISTING`),
    // so the old remove-then-rename fallback was both unnecessary and lossy.
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err).context("failed to replace board file");
    }
    Ok(())
}

/// Parse raw board bytes, for a caller that merges a board read elsewhere (the
/// sync layer reads the remote copy). `None` when the bytes are not a board.
pub fn board_from_bytes(bytes: &[u8]) -> Option<Board> {
    let schema = serde_json::from_slice::<FileSchema>(bytes).ok()?;
    Some(Board::from_tasks(schema.tasks))
}

/// Serialize a board to the on-disk file shape. The one place that knows that
/// layout, so `Store::save` and `merge_files` cannot drift apart.
fn serialize(board: &Board) -> serde_json::Result<Vec<u8>> {
    let schema = FileSchema {
        tasks: board.tasks().to_vec(),
    };
    serde_json::to_vec_pretty(&schema)
}

/// Which side of a [`merge_files`] call could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeSide {
    Base,
    Other,
}

/// Merge two serialized boards: `base` wins ties, `other` fills in and advances
/// matching tasks. Returns `Err` naming the unreadable side, so the caller can
/// surface it instead of silently dropping that side's tasks.
pub fn merge_files(base: &[u8], other: &[u8]) -> Result<Vec<u8>, MergeSide> {
    let base = board_from_bytes(base).ok_or(MergeSide::Base)?;
    let other = board_from_bytes(other).ok_or(MergeSide::Other)?;
    // Serializing our own board cannot fail; label it as `Base` only so the
    // type is total.
    serialize(&base.merged_with(&other)).map_err(|_| MergeSide::Base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_time::at;

    fn board_bytes(tasks: &[(&str, u32)]) -> Vec<u8> {
        let schema = FileSchema {
            tasks: tasks
                .iter()
                .map(|(title, day)| Task::new(*title, at(2026, 10, *day)))
                .collect(),
        };
        serde_json::to_vec(&schema).unwrap()
    }

    #[test]
    fn merging_files_matches_tasks_by_creation_time() {
        let base = board_bytes(&[("shared", 5), ("base only", 6)]);
        let other = board_bytes(&[("shared", 5), ("other only", 7)]);
        let merged: FileSchema =
            serde_json::from_slice(&merge_files(&base, &other).unwrap()).unwrap();
        let titles: Vec<&str> = merged.tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["shared", "base only", "other only"]);
    }

    #[test]
    fn a_remote_board_with_duplicate_copies_is_collapsed_on_read() {
        let twin = FileSchema {
            tasks: vec![
                Task::new("twin", at(2026, 10, 5)),
                Task::new("twin", at(2026, 10, 5)),
            ],
        };
        let bytes = serde_json::to_vec(&twin).unwrap();
        assert_eq!(board_from_bytes(&bytes).unwrap().len(), 1);
    }

    #[test]
    fn merging_names_the_side_it_cannot_read() {
        let ok = board_bytes(&[("a", 5)]);
        assert_eq!(merge_files(b"not json", &ok), Err(MergeSide::Base));
        assert_eq!(merge_files(&ok, b"neither is this"), Err(MergeSide::Other));
    }

    fn store_in(dir: &tempfile::TempDir) -> Store {
        Store::new(dir.path().join("todos.json"))
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = store_in(&dir).load();
        assert!(loaded.board.is_empty());
        assert!(loaded.damaged.is_none(), "a missing file is not damage");
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

        assert_eq!(loaded.board.len(), 2);
        assert_eq!(loaded.board.task(0).unwrap().title, "Nand2Tetris 第六章");
        assert_eq!(loaded.board.task(1).unwrap().title, "学习 GPUI");
        assert!(loaded.damaged.is_none());
    }

    #[test]
    fn writes_a_tasks_array() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.save(&Board::new()).unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(store.path()).unwrap()).unwrap();
        assert!(value.get("tasks").is_some_and(serde_json::Value::is_array));
    }

    #[test]
    fn corrupt_file_is_backed_up_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        std::fs::write(store.path(), "{ not json").unwrap();

        let loaded = store.load();
        assert!(loaded.board.is_empty());

        // The damage is reported with where the original went, so a caller can
        // say so instead of appearing to have lost the board.
        let damage = loaded.damaged.expect("a corrupt file is damaged");
        assert!(damage.backup.is_some(), "the original was moved aside");

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

        assert_eq!(store.load().board.len(), 1);
        assert_eq!(store.load().board.task(0).unwrap().title, "new");
    }

    #[test]
    fn mtime_is_available_once_saved() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        assert!(store.mtime().is_none());
        store.save(&Board::new()).unwrap();
        assert!(store.mtime().is_some());
    }

    #[test]
    fn raw_bytes_roundtrip_through_the_atomic_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        assert!(
            store.read_bytes().unwrap().is_none(),
            "a missing file reads as None"
        );

        store.write_bytes(b"{\"tasks\":[]}").unwrap();
        assert_eq!(
            store.read_bytes().unwrap().as_deref(),
            Some(&b"{\"tasks\":[]}"[..])
        );
    }

    #[test]
    fn unreadable_file_is_an_error_not_a_missing_board() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        // A directory in the file's place cannot be read; that must be `Err`,
        // not `Ok(None)`, or a merge would treat it as an absent board.
        std::fs::create_dir(store.path()).unwrap();
        assert!(store.read_bytes().is_err());
    }
}
