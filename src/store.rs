use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::todo::Todo;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub todos: Vec<Todo>,
}

impl Store {
    fn path() -> Option<PathBuf> {
        dirs::data_dir().map(|dir| dir.join(env!("CARGO_PKG_NAME")).join("todos.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}
