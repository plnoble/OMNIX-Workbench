//! Configuration Backup System
//!
//! Creates timestamped backups before any configuration modification.
//! Backup location: `storage::backups_dir()/<category>/` (user-configurable).

use std::fs;
use std::path::PathBuf;

/// Create a timestamped backup of a file before modification
pub fn backup_file(file_path: &PathBuf, category: &str) -> Result<Option<PathBuf>, String> {
    if !file_path.exists() {
        return Ok(None); // Nothing to backup
    }

    let backup_dir = crate::storage::backups_dir().join(category);
    fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let file_name = file_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".into());
    let backup_name = format!("{}_{}", timestamp, file_name);
    let backup_path = backup_dir.join(&backup_name);

    fs::copy(file_path, &backup_path).map_err(|e| format!("Backup failed: {}", e))?;

    // Cleanup old backups (keep last 20 per category)
    cleanup_old_backups(&backup_dir, 20)?;

    Ok(Some(backup_path))
}

/// Create a backup of a string content (for in-memory configs)
pub fn backup_content(content: &str, category: &str, filename: &str) -> Result<PathBuf, String> {
    let backup_dir = crate::storage::backups_dir().join(category);
    fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("{}_{}", timestamp, filename);
    let backup_path = backup_dir.join(&backup_name);

    fs::write(&backup_path, content).map_err(|e| format!("Backup failed: {}", e))?;

    cleanup_old_backups(&backup_dir, 20)?;
    Ok(backup_path)
}

/// List backups for a category
pub fn list_backups(category: &str) -> Vec<BackupEntry> {
    let backup_dir = crate::storage::backups_dir().join(category);
    if !backup_dir.exists() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    if let Ok(dir) = fs::read_dir(&backup_dir) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let modified = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                entries.push(BackupEntry {
                    name,
                    path: path.to_string_lossy().to_string(),
                    size_bytes: size,
                    created_at: modified,
                });
            }
        }
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    entries
}

/// Restore a backup file to its original location
pub fn restore_backup(backup_path: &str, target_path: &str) -> Result<(), String> {
    let src = PathBuf::from(backup_path);
    let dst = PathBuf::from(target_path);

    if !src.exists() {
        return Err(format!("Backup file not found: {}", backup_path));
    }

    fs::copy(&src, &dst).map_err(|e| format!("Restore failed: {}", e))?;
    Ok(())
}

/// Remove old backups, keeping the N most recent
fn cleanup_old_backups(dir: &PathBuf, keep: usize) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_file())
        .collect();

    entries.sort_by(|a, b| {
        b.metadata().and_then(|m| m.modified()).ok()
            .cmp(&a.metadata().and_then(|m| m.modified()).ok())
    });

    for entry in entries.iter().skip(keep) {
        let _ = fs::remove_file(entry.path());
    }

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub created_at: u64,
}
