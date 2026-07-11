//! Write — a Markdown writing workspace.
//!
//! A "space" is a folder of `.md` files. The default space lives at
//! `~/.omnix/write`; users can add custom folders (persisted in the
//! `write_spaces` setting). All file IO is guarded to stay inside the chosen
//! space (lexical containment, `.md` only). Markdown → HTML export saves a
//! styled, self-contained HTML file the frontend assembles from its preview.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::input_validation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteSpace {
    pub name: String,
    pub path: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFile {
    pub name: String,
    /// Path relative to the space root.
    pub relative_path: String,
    pub updated_at: String,
}

fn default_space_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
    let dir = home.join(".omnix").join("write");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Canonicalizes and validates a space path, and confirms it is a directory.
fn normalize_space(space_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(space_path);
    if !path.is_dir() {
        return Err(format!("写作空间不存在或不是目录：{space_path}"));
    }
    path.canonicalize().map_err(|e| e.to_string())
}

/// Resolves a space-relative `.md` file path, rejecting traversal / non-md.
fn resolve_md(space: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let rel = relative_path.replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.contains("..") {
        return Err("非法的文件路径".into());
    }
    if !rel.ends_with(".md") {
        return Err("只能操作 .md 文件".into());
    }
    Ok(space.join(rel))
}

fn sanitize_filename(name: &str) -> Result<String, String> {
    let trimmed = name.trim().trim_end_matches(".md");
    if trimmed.is_empty() {
        return Err("文件名不能为空".into());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err("文件名不能包含路径分隔符".into());
    }
    Ok(format!("{trimmed}.md"))
}

fn file_mtime_iso(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|time| {
            let dt: chrono::DateTime<chrono::Local> = time.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_default()
}

fn load_custom_spaces(db: &DbManager) -> Vec<WriteSpace> {
    let raw = db
        .get_setting("write_spaces")
        .ok()
        .flatten()
        .unwrap_or_default();
    serde_json::from_str::<Vec<WriteSpace>>(&raw).unwrap_or_default()
}

/// Lists writing spaces: the default plus any custom folders.
#[tauri::command]
pub fn write_list_spaces(db: State<'_, Arc<DbManager>>) -> Result<Vec<WriteSpace>, String> {
    let mut spaces = vec![WriteSpace {
        name: "默认写作空间".into(),
        path: default_space_dir()?.to_string_lossy().to_string(),
        is_default: true,
    }];
    spaces.extend(load_custom_spaces(&db).into_iter().map(|mut s| {
        s.is_default = false;
        s
    }));
    Ok(spaces)
}

/// Adds a custom writing space (an existing folder).
#[tauri::command]
pub fn write_add_space(path: String, db: State<'_, Arc<DbManager>>) -> Result<WriteSpace, String> {
    input_validation::validate_workspace_path(&path, "path")?;
    let canonical = normalize_space(&path)?;
    let path_str = canonical.to_string_lossy().to_string();
    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("写作空间")
        .to_string();
    let mut custom = load_custom_spaces(&db);
    if !custom.iter().any(|s| s.path == path_str) {
        custom.push(WriteSpace { name: name.clone(), path: path_str.clone(), is_default: false });
        db.set_setting("write_spaces", &serde_json::to_string(&custom).unwrap_or_default())
            .map_err(|e| e.to_string())?;
    }
    Ok(WriteSpace { name, path: path_str, is_default: false })
}

/// Removes a custom writing space from the list (does not delete files).
#[tauri::command]
pub fn write_remove_space(path: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let mut custom = load_custom_spaces(&db);
    custom.retain(|s| s.path != path);
    db.set_setting("write_spaces", &serde_json::to_string(&custom).unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Lists `.md` files in a space (newest first).
#[tauri::command]
pub fn write_list_files(space_path: String) -> Result<Vec<WriteFile>, String> {
    let space = normalize_space(&space_path)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(&space).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        files.push(WriteFile {
            relative_path: name.clone(),
            name,
            updated_at: file_mtime_iso(&path),
        });
    }
    files.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(files)
}

#[tauri::command]
pub fn write_read_file(space_path: String, relative_path: String) -> Result<String, String> {
    let space = normalize_space(&space_path)?;
    let path = resolve_md(&space, &relative_path)?;
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_save_file(space_path: String, relative_path: String, content: String) -> Result<(), String> {
    let space = normalize_space(&space_path)?;
    let path = resolve_md(&space, &relative_path)?;
    fs::write(&path, content).map_err(|e| e.to_string())
}

/// Creates a new empty `.md` file and returns its relative path.
#[tauri::command]
pub fn write_create_file(space_path: String, name: String) -> Result<String, String> {
    let space = normalize_space(&space_path)?;
    let file_name = sanitize_filename(&name)?;
    let path = space.join(&file_name);
    if path.exists() {
        return Err("同名文件已存在".into());
    }
    fs::write(&path, format!("# {}\n\n", name.trim().trim_end_matches(".md"))).map_err(|e| e.to_string())?;
    Ok(file_name)
}

#[tauri::command]
pub fn write_rename_file(space_path: String, relative_path: String, new_name: String) -> Result<String, String> {
    let space = normalize_space(&space_path)?;
    let from = resolve_md(&space, &relative_path)?;
    let file_name = sanitize_filename(&new_name)?;
    let to = space.join(&file_name);
    if to.exists() {
        return Err("同名文件已存在".into());
    }
    fs::rename(&from, &to).map_err(|e| e.to_string())?;
    Ok(file_name)
}

#[tauri::command]
pub fn write_delete_file(space_path: String, relative_path: String) -> Result<(), String> {
    let space = normalize_space(&space_path)?;
    let path = resolve_md(&space, &relative_path)?;
    fs::remove_file(&path).map_err(|e| e.to_string())
}

/// Saves a frontend-assembled HTML export next to the source `.md`, returning
/// the absolute path of the written `.html` file.
#[tauri::command]
pub fn write_export_html(space_path: String, relative_path: String, html: String) -> Result<String, String> {
    let space = normalize_space(&space_path)?;
    let md_path = resolve_md(&space, &relative_path)?;
    let html_path = md_path.with_extension("html");
    fs::write(&html_path, html).map_err(|e| e.to_string())?;
    Ok(html_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_non_md() {
        let tmp = std::env::temp_dir();
        assert!(resolve_md(&tmp, "../secret.md").is_err());
        assert!(resolve_md(&tmp, "notes.txt").is_err());
        assert!(resolve_md(&tmp, "/abs.md").is_err());
        assert!(resolve_md(&tmp, "ok.md").is_ok());
    }

    #[test]
    fn sanitizes_filenames() {
        assert_eq!(sanitize_filename("draft").unwrap(), "draft.md");
        assert_eq!(sanitize_filename("draft.md").unwrap(), "draft.md");
        assert!(sanitize_filename("a/b").is_err());
        assert!(sanitize_filename("..").is_err());
        assert!(sanitize_filename("  ").is_err());
    }
}
