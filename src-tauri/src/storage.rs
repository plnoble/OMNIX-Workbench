//! Configurable storage locations (R1 存储位置中心).
//!
//! Users can point heavy/growing directories (backups, exports, the central
//! skill store) anywhere — e.g. off the C: drive. Overrides live in the
//! `settings` table and are mirrored into a process-wide cache at startup so
//! stateless helpers (backup.rs etc.) can resolve directories without a DB
//! handle. The DB itself and media stay at `~/.omnix` (fixed contract: SQLite
//! path + the asset-protocol scope in tauri.conf.json can't move at runtime).
//! The agents install root is the existing `sandbox_dir` setting — it is read
//! from the DB directly by agent.rs, so it needs no cache entry here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::db::DbManager;

/// (setting key, default subdirectory under ~/.omnix, 中文标签)
pub const STORAGE_KEYS: &[(&str, &str, &str)] = &[
    ("storage_backups_dir", "backups", "备份目录"),
    ("storage_exports_dir", "exports", "导出目录"),
    ("storage_skills_dir", "skills", "技能中央库"),
];

fn overrides() -> &'static RwLock<HashMap<String, String>> {
    static CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Load overrides from settings once at startup (lib.rs setup).
pub fn init_from_db(db: &DbManager) {
    let mut map = HashMap::new();
    for (key, _, _) in STORAGE_KEYS {
        if let Ok(Some(value)) = db.get_setting(key) {
            if !value.trim().is_empty() {
                map.insert((*key).to_string(), value);
            }
        }
    }
    if let Ok(mut cache) = overrides().write() {
        *cache = map;
    }
}

/// Update the cache after the user changes a location (empty = back to default).
pub fn set_override(key: &str, value: &str) {
    if let Ok(mut cache) = overrides().write() {
        if value.trim().is_empty() {
            cache.remove(key);
        } else {
            cache.insert(key.to_string(), value.to_string());
        }
    }
}

pub fn omnix_root() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".omnix")
}

pub fn default_dir(key: &str) -> PathBuf {
    let sub = STORAGE_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, s, _)| *s)
        .unwrap_or("misc");
    omnix_root().join(sub)
}

/// Resolve a storage dir: user override if set, else the ~/.omnix default.
pub fn dir_for(key: &str) -> PathBuf {
    if let Ok(cache) = overrides().read() {
        if let Some(v) = cache.get(key) {
            return PathBuf::from(v);
        }
    }
    default_dir(key)
}

pub fn backups_dir() -> PathBuf {
    dir_for("storage_backups_dir")
}

pub fn exports_dir() -> PathBuf {
    dir_for("storage_exports_dir")
}

pub fn skills_dir() -> PathBuf {
    dir_for("storage_skills_dir")
}

/// Recursively copy a directory tree (shared by backup/migration/skill ops).
pub(crate) fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_and_empty_resets() {
        set_override("storage_exports_dir", "D:/somewhere/exports");
        assert_eq!(
            dir_for("storage_exports_dir"),
            PathBuf::from("D:/somewhere/exports")
        );
        set_override("storage_exports_dir", "  ");
        assert_eq!(dir_for("storage_exports_dir"), default_dir("storage_exports_dir"));
    }
}
