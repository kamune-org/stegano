//! Process-wide command state: jobs, revealed entries, the held hide output,
//! and in-progress imports.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use stegano_core::Entry;
use tauri::{Manager, Runtime};
use zeroize::Zeroize;

use crate::jobs::{Guard, Jobs};

pub struct Held {
    pub bytes: Vec<u8>,
    /// Suggested download name. `hide` already returned it; `save_output`
    /// writes `bytes` to a path the frontend chose.
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub extension: String,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

pub struct Import {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

pub struct AppState {
    pub jobs: Arc<Jobs>,
    revealed: Mutex<Vec<Entry>>,
    held: Mutex<Option<Held>>,
    imports: Mutex<HashMap<String, Import>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            jobs: Arc::new(Jobs::default()),
            revealed: Mutex::new(Vec::new()),
            held: Mutex::new(None),
            imports: Mutex::new(HashMap::new()),
        }
    }
}

impl AppState {
    pub fn begin_job(&self, id: &str) -> Guard {
        self.jobs.begin(id)
    }

    pub fn clear_revealed(&self) {
        let mut guard = self.revealed.lock().unwrap_or_else(|p| p.into_inner());
        for entry in guard.iter_mut() {
            entry.data.zeroize();
        }
        guard.clear();
    }

    pub fn set_revealed(&self, entries: Vec<Entry>) {
        self.clear_revealed();
        *self.revealed.lock().unwrap_or_else(|p| p.into_inner()) = entries;
    }

    pub fn revealed_bytes(&self, index: usize) -> Option<Vec<u8>> {
        self.revealed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(index)
            .map(|entry| entry.data.clone())
    }

    pub fn clear_held(&self) {
        let _old = self.held.lock().unwrap_or_else(|p| p.into_inner()).take();
    }

    pub fn set_held(&self, held: Held) {
        let _old = self
            .held
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .replace(held);
    }

    pub fn held_bytes(&self) -> Option<Vec<u8>> {
        self.held
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|held| held.bytes.clone())
    }

    pub fn insert_import(&self, id: String, import: Import) {
        self.imports
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, import);
    }

    pub fn with_import_mut<T>(
        &self,
        id: &str,
        f: impl FnOnce(&mut Import) -> Result<T, crate::error::CommandError>,
    ) -> Result<T, crate::error::CommandError> {
        let mut imports = self.imports.lock().unwrap_or_else(|p| p.into_inner());
        let import = imports
            .get_mut(id)
            .ok_or_else(|| crate::error::CommandError::Other("unknown import".into()))?;
        f(import)
    }

    pub fn take_import(&self, id: &str) -> Option<Import> {
        self.imports
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id)
    }
}

/// Delete cached work files from the previous run, then recreate the directory.
pub fn reset_work<R: Runtime>(app: &impl Manager<R>) {
    if let Ok(cache) = app.path().app_cache_dir() {
        let work = cache.join("work");
        let _ = std::fs::remove_dir_all(&work);
        let _ = std::fs::create_dir_all(&work);
    }
}
