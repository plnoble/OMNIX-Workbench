//! Storage location commands (R1 存储位置中心).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageLocation {
    pub key: String,
    pub label: String,
    pub path: String,
    pub default_path: String,
    pub is_default: bool,
}

/// All configurable locations, including the agents install root
/// (`sandbox_dir`, read directly from settings by agent.rs).
#[tauri::command]
pub fn get_storage_config(db: State<'_, Arc<DbManager>>) -> Result<Vec<StorageLocation>, String> {
    let mut out = Vec::new();
    for (key, _, label) in storage::STORAGE_KEYS {
        let default_path = storage::default_dir(key).to_string_lossy().to_string();
        let path = storage::dir_for(key).to_string_lossy().to_string();
        out.push(StorageLocation {
            key: (*key).to_string(),
            label: (*label).to_string(),
            path: path.clone(),
            default_path: default_path.clone(),
            is_default: path == default_path,
        });
    }
    // Agents install root (managed CLI copies) — existing sandbox_dir setting.
    let agents_default = storage::omnix_root().join("agents");
    let agents_current = db
        .get_setting("sandbox_dir")
        .unwrap_or(None)
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| agents_default.clone());
    out.push(StorageLocation {
        key: "sandbox_dir".to_string(),
        label: "Agent 安装目录".to_string(),
        path: agents_current.to_string_lossy().to_string(),
        default_path: agents_default.to_string_lossy().to_string(),
        is_default: agents_current == agents_default,
    });
    Ok(out)
}

fn validate_dir(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path.trim());
    if !p.is_absolute() {
        return Err("请输入绝对路径（例如 D:\\OMNIX\\backups）".to_string());
    }
    std::fs::create_dir_all(&p).map_err(|e| format!("目录不可用: {e}"))?;
    Ok(p)
}

/// Set (or reset with empty path) a storage location. For the skills central
/// store use `migrate_skills_store` instead — it must move data too.
#[tauri::command]
pub fn set_storage_dir(
    key: String,
    path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let known = storage::STORAGE_KEYS.iter().any(|(k, _, _)| *k == key) || key == "sandbox_dir";
    if !known {
        return Err(format!("未知存储项: {key}"));
    }
    if key == "storage_skills_dir" && !path.trim().is_empty() {
        return Err("技能中央库请用「迁移」按钮改位置（需要搬移文件并更新索引）".to_string());
    }
    let value = if path.trim().is_empty() {
        String::new()
    } else {
        validate_dir(&path)?.to_string_lossy().to_string()
    };
    db.set_setting(&key, &value).map_err(|e| e.to_string())?;
    if key != "sandbox_dir" {
        storage::set_override(&key, &value);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMigrationReport {
    pub moved: usize,
    pub new_dir: String,
    pub old_dir: String,
    pub errors: Vec<String>,
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
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

/// Move the central skill store to a new directory: copy every skill folder,
/// rewrite `central_path`/`file_path` in the DB, then persist the setting.
/// The old directory is only removed after everything copied cleanly.
#[tauri::command]
pub fn migrate_skills_store(
    new_dir: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillsMigrationReport, String> {
    let new_root = validate_dir(&new_dir)?;
    let old_root = storage::skills_dir();
    if new_root == old_root {
        return Err("新位置与当前位置相同".to_string());
    }
    if new_root.starts_with(&old_root) {
        return Err("新位置不能在当前技能库内部".to_string());
    }

    let mut moved = 0usize;
    let mut errors = Vec::new();
    if old_root.exists() {
        for entry in std::fs::read_dir(&old_root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let dest = new_root.join(entry.file_name());
            match copy_dir_recursive(&entry.path(), &dest) {
                Ok(()) => moved += 1,
                Err(e) => errors.push(format!("{}: {e}", entry.file_name().to_string_lossy())),
            }
        }
    }
    if !errors.is_empty() {
        // Nothing destructive happened yet — the old store stays authoritative.
        return Err(format!("迁移中断，{} 个技能复制失败（原库未动）: {}", errors.len(), errors[0]));
    }

    // Rewrite DB paths (prefix replace) and persist the new location.
    let old_prefix = old_root.to_string_lossy().to_string();
    let new_prefix = new_root.to_string_lossy().to_string();
    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE skills SET
                central_path = REPLACE(central_path, ?1, ?2),
                file_path = REPLACE(file_path, ?1, ?2)",
            rusqlite::params![old_prefix, new_prefix],
        )
        .map_err(|e| e.to_string())?;
    }
    db.set_setting("storage_skills_dir", &new_prefix)
        .map_err(|e| e.to_string())?;
    storage::set_override("storage_skills_dir", &new_prefix);

    // Copies verified + DB switched — retire the old store.
    let _ = std::fs::remove_dir_all(&old_root);

    Ok(SkillsMigrationReport {
        moved,
        new_dir: new_prefix,
        old_dir: old_prefix,
        errors,
    })
}
