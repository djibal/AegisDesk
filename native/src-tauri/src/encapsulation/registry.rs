use std::{fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::NetworkPolicy;
use crate::error::AegisError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedApp {
    pub id: String,
    pub name: String,
    pub url: String,
    pub host: String,
    /// Ephemeral apps keep nothing on disk after the window closes.
    pub ephemeral: bool,
    pub network: NetworkPolicy,
}

/// Small JSON registry in the app data dir. Kept in memory (a few KB).
pub struct Registry {
    file: PathBuf,
    root: PathBuf,
    pub apps: Mutex<Vec<WrappedApp>>,
}

impl Registry {
    pub fn load(handle: &AppHandle) -> Result<Self, AegisError> {
        let root = handle.path().app_data_dir().map_err(|_| AegisError::Storage)?;
        fs::create_dir_all(root.join("sessions"))?;
        let file = root.join("apps.json");
        let apps = match fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Ok(Self { file, root, apps: Mutex::new(apps) })
    }

    pub fn save(&self, apps: &[WrappedApp]) -> Result<(), AegisError> {
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(apps)?)?;
        fs::rename(tmp, &self.file)?; // atomic replace
        Ok(())
    }

    /// One storage partition per app: cookies, localStorage, IndexedDB,
    /// cache and service workers never cross apps.
    pub fn session_dir(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        self.root.join("sessions").join(safe)
    }

    pub fn get(&self, id: &str) -> Result<WrappedApp, AegisError> {
        self.apps.lock().unwrap().iter().find(|a| a.id == id).cloned().ok_or(AegisError::NotFound)
    }
}
