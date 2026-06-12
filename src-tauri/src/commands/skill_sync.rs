use tauri::State;
use std::sync::Arc;
use std::path::PathBuf;
use std::io::{Read as IoRead, Write as IoWrite};
use rusqlite::params;
use crate::db::DbManager;
use crate::tool_adapters::{AdapterRegistry, SyncMode, ToolStatus, DiscoveredSkill, SyncResult};
use crate::sync_engine::{SyncEngine, ConflictInfo, ConflictStrategy, DetailedSyncResult, BatchSyncResult, DriftReport, ScanReport, ScanItem};
use crate::sync_engine::{GitSkillCandidate, GitCloneResult, GitUpdateCheck};
use super::*;

// ══════════════════════════════════════════════════
// Skill Sync Commands (P1 — DEC-018)
// ══════════════════════════════════════════════════

/// Skill target record returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTargetRecord {
    pub id: String,
    pub skill_id: String,
    pub tool: String,
    pub target_path: String,
    pub mode: String,
    pub status: String,
    pub last_error: Option<String>,
    pub synced_at: Option<i64>,
}

/// Get all tool adapters and their installation status
#[tauri::command]
pub fn get_skill_tool_status() -> Vec<ToolStatus> {
    let registry = AdapterRegistry::new();
    registry.tool_status_list()
}

/// Sync a skill to one or more tools
#[tauri::command]
pub fn sync_skill_to_tools(
    skill_name: String,
    tool_ids: Vec<String>,
    mode: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SyncResult>, String> {
    let sync_mode = match mode.as_str() {
        "symlink" => SyncMode::Symlink,
        _ => SyncMode::Copy,
    };

    // Read skill content from central store
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = conn
        .query_row(
            "SELECT file_path FROM skills WHERE name = ?1",
            params![skill_name],
            |r| r.get(0),
        )
        .map_err(|e| format!("Skill not found: {}", e))?;

    // Read the core profile content
    let mut core_path = PathBuf::from(&file_path_str);
    core_path.set_file_name(format!("{}_core.md", skill_name));
    let content = std::fs::read_to_string(&core_path)
        .map_err(|e| format!("Failed to read skill content: {}", e))?;

    let registry = AdapterRegistry::new();
    let mut results = Vec::new();

    for tool_id in &tool_ids {
        if let Some(adapter) = registry.get(tool_id) {
            let result = adapter.sync_skill(&skill_name, &content, &sync_mode);

            // Update skill_targets table
            let status_str = if result.success { "synced" } else { "error" };
            let _ = conn.execute(
                "INSERT OR REPLACE INTO skill_targets (id, skill_id, tool, target_path, mode, status, last_error, synced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%s','now'))",
                params![
                    format!("{}-{}", skill_name, tool_id),
                    skill_name,
                    tool_id,
                    result.target_path,
                    mode,
                    status_str,
                    result.error.as_deref().unwrap_or(""),
                ],
            );

            results.push(result);
        }
    }

    Ok(results)
}

/// Unsync (remove) a skill from a tool's directory
#[tauri::command]
pub fn unsync_skill_from_tool(
    skill_name: String,
    tool_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<SyncResult, String> {
    let registry = AdapterRegistry::new();
    let adapter = registry.get(&tool_id)
        .ok_or_else(|| format!("Unknown tool: {}", tool_id))?;

    let result = adapter.unsync_skill(&skill_name);

    // Remove from skill_targets table
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let _ = conn.execute(
        "DELETE FROM skill_targets WHERE skill_id = ?1 AND tool = ?2",
        params![skill_name, tool_id],
    );

    Ok(result)
}

/// Scan all tool directories for existing skills
#[tauri::command]
pub fn scan_all_tool_skills() -> Vec<DiscoveredSkill> {
    let registry = AdapterRegistry::new();
    let mut all_skills = Vec::new();
    for adapter in registry.all() {
        all_skills.extend(adapter.list_skills());
    }
    all_skills
}

/// Toggle skill starred status
#[tauri::command]
pub fn toggle_skill_starred(
    skill_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET starred = CASE WHEN starred = 0 THEN 1 ELSE 0 END WHERE name = ?1",
        params![skill_name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// Get sync targets for a specific skill
#[tauri::command]
pub fn get_skill_targets(
    skill_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SkillTargetRecord>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, skill_id, tool, target_path, mode, status, last_error, synced_at FROM skill_targets WHERE skill_id = ?1"
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt.query_map(params![skill_name], |row| {
        Ok(SkillTargetRecord {
            id: row.get(0)?,
            skill_id: row.get(1)?,
            tool: row.get(2)?,
            target_path: row.get(3)?,
            mode: row.get(4)?,
            status: row.get(5)?,
            last_error: row.get(6)?,
            synced_at: row.get(7)?,
        })
    }).map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(item) = r { result.push(item); }
    }
    Ok(result)
}

// ══════════════════════════════════════════════════
// Skill Sync Engine Commands (P2 — DEC-018)
// ══════════════════════════════════════════════════

/// Check for conflicts before syncing a skill
#[tauri::command]
pub fn check_sync_conflicts(
    skill_name: String,
    tool_ids: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Vec<ConflictInfo> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.check_conflicts(&skill_name, &tool_ids)
}

/// Sync one skill to one tool with conflict strategy
#[tauri::command]
pub fn sync_skill_detailed(
    skill_name: String,
    tool_id: String,
    mode: String,
    strategy: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DetailedSyncResult, String> {
    let sync_mode = match mode.as_str() {
        "symlink" => SyncMode::Symlink,
        _ => SyncMode::Copy,
    };
    let conflict_strategy = match strategy.as_str() {
        "skip" => ConflictStrategy::Skip,
        "rename" => ConflictStrategy::Rename,
        _ => ConflictStrategy::Overwrite,
    };

    let engine = SyncEngine::new(Arc::clone(&db));
    Ok(engine.sync_one(&skill_name, &tool_id, &sync_mode, &conflict_strategy))
}

/// Sync one skill to multiple tools
#[tauri::command]
pub fn sync_skill_to_many(
    skill_name: String,
    tool_ids: Vec<String>,
    mode: String,
    strategy: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<BatchSyncResult, String> {
    let sync_mode = match mode.as_str() {
        "symlink" => SyncMode::Symlink,
        _ => SyncMode::Copy,
    };
    let conflict_strategy = match strategy.as_str() {
        "skip" => ConflictStrategy::Skip,
        "rename" => ConflictStrategy::Rename,
        _ => ConflictStrategy::Overwrite,
    };

    let engine = SyncEngine::new(Arc::clone(&db));
    Ok(engine.sync_one_to_many(&skill_name, &tool_ids, &sync_mode, &conflict_strategy))
}

/// Batch sync: sync multiple skills to all installed tools
#[tauri::command]
pub fn sync_skills_batch(
    skill_names: Vec<String>,
    mode: String,
    strategy: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<BatchSyncResult>, String> {
    let sync_mode = match mode.as_str() {
        "symlink" => SyncMode::Symlink,
        _ => SyncMode::Copy,
    };
    let conflict_strategy = match strategy.as_str() {
        "skip" => ConflictStrategy::Skip,
        "rename" => ConflictStrategy::Rename,
        _ => ConflictStrategy::Overwrite,
    };

    let engine = SyncEngine::new(Arc::clone(&db));
    Ok(engine.sync_batch(&skill_names, &sync_mode, &conflict_strategy))
}

/// Check drift for a specific skill+tool
#[tauri::command]
pub fn check_skill_drift(
    skill_name: String,
    tool_id: String,
    db: State<'_, Arc<DbManager>>,
) -> DriftReport {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.check_drift(&skill_name, &tool_id)
}

/// Check drift for all synced skills
#[tauri::command]
pub fn check_all_drift(
    db: State<'_, Arc<DbManager>>,
) -> Vec<DriftReport> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.check_all_drift()
}

/// Re-sync all skills that have drifted
#[tauri::command]
pub fn resync_all_drifted(
    mode: String,
    db: State<'_, Arc<DbManager>>,
) -> Vec<DetailedSyncResult> {
    let sync_mode = match mode.as_str() {
        "symlink" => SyncMode::Symlink,
        _ => SyncMode::Copy,
    };
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.resync_drifted(&sync_mode)
}

// ══════════════════════════════════════════════════
// Disk Scanner Commands (P4 — DEC-018)
// ══════════════════════════════════════════════════

/// Scan all tool directories and classify every discovered skill
#[tauri::command]
pub fn scan_disk_skills(
    db: State<'_, Arc<DbManager>>,
) -> ScanReport {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.scan_disk_skills()
}

/// Import unmanaged skills into the OMNIX database
#[tauri::command]
pub fn import_unmanaged_skills(
    items: Vec<ScanItem>,
    db: State<'_, Arc<DbManager>>,
) -> Result<usize, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.import_unmanaged(&items)
}

// ══════════════════════════════════════════════════
// Skill Package & Category Commands (P6 — DEC-018)
// ══════════════════════════════════════════════════

/// Export a single skill as a .zip package to ~/.omnix/exports/
#[tauri::command]
pub fn export_skill_package(
    skill_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Read skill metadata
    let (description, file_path_str): (String, String) = conn
        .query_row(
            "SELECT description, file_path FROM skills WHERE name = ?1",
            params![skill_name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("Skill '{}' not found: {}", skill_name, e))?;

    let base_dir = PathBuf::from(&file_path_str);
    if !base_dir.exists() {
        return Err(format!("Skill directory not found: {}", file_path_str));
    }

    // Ensure exports directory exists
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let exports_dir = home.join(".omnix").join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;

    let zip_path = exports_dir.join(format!("{}.skill", skill_name));

    // Create zip archive
    let file = std::fs::File::create(&zip_path).map_err(|e| format!("Failed to create zip: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Write metadata
    let metadata = serde_json::json!({
        "name": skill_name,
        "description": description,
        "version": "1.0",
        "exported_at": chrono::Utc::now().to_rfc3339(),
    });
    zip.start_file("metadata.json", options).map_err(|e| format!("Zip write error: {}", e))?;
    zip.write_all(serde_json::to_string_pretty(&metadata).expect("metadata serialization should not fail").as_bytes()).map_err(|e| format!("Zip write error: {}", e))?;

    // Add all .md files from the skill directory
    if base_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let file_name = path.file_name().expect("entry should have a file name").to_string_lossy().to_string();
                    let content = std::fs::read_to_string(&path).map_err(|e| format!("Read {} failed: {}", file_name, e))?;
                    zip.start_file(&file_name, options).map_err(|e| format!("Zip write error: {}", e))?;
                    zip.write_all(content.as_bytes()).map_err(|e| format!("Zip write error: {}", e))?;
                }
            }
        }
    }

    zip.finish().map_err(|e| format!("Zip finalize error: {}", e))?;

    Ok(zip_path.to_string_lossy().to_string())
}

/// Import a skill from a .zip or .skill package
#[tauri::command]
pub fn import_skill_package(
    zip_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let path = PathBuf::from(&zip_path);
    if !path.exists() {
        return Err(format!("File not found: {}", zip_path));
    }

    let file = std::fs::File::open(&path).map_err(|e| format!("Failed to open: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip: {}", e))?;

    // Read metadata
    let mut metadata_str = String::new();
    if let Ok(mut meta_file) = archive.by_name("metadata.json") {
        meta_file.read_to_string(&mut metadata_str).map_err(|e| e.to_string())?;
    }
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str).unwrap_or(serde_json::json!({}));

    let skill_name = metadata["name"].as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.file_stem().expect("import path should have a file stem").to_string_lossy().to_string());
    let description = metadata["description"].as_str().unwrap_or("Imported skill").to_string();

    // Create central store directory
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let skills_dir = home.join(".omnix").join("skills");
    std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;

    let central_dir = skills_dir.join(&skill_name);
    std::fs::create_dir_all(&central_dir).map_err(|e| e.to_string())?;

    // Extract all .md files
    let mut has_skill_md = false;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Zip read error: {}", e))?;
        let name = file.name().to_string();
        if name == "metadata.json" { continue; }
        if !name.ends_with(".md") { continue; }

        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| e.to_string())?;

        let out_path = central_dir.join(&name);
        std::fs::write(&out_path, &content).map_err(|e| format!("Write {} failed: {}", name, e))?;

        if name == "SKILL.md" { has_skill_md = true; }
    }

    // If no SKILL.md, create one from the first .md file found
    if !has_skill_md {
        let core_path = central_dir.join(format!("{}_core.md", skill_name));
        if core_path.exists() {
            let content = std::fs::read_to_string(&core_path).map_err(|e| e.to_string())?;
            std::fs::write(central_dir.join("SKILL.md"), &content).map_err(|e| e.to_string())?;
        }
    }

    // Insert into database
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let central_path_str = central_dir.to_string_lossy().to_string();

    conn.execute(
        "INSERT OR REPLACE INTO skills (name, description, file_path, profile, is_active, dependencies, source_type, source_ref, central_path)
         VALUES (?1, ?2, ?3, 'Core', 1, '[]', 'local', ?4, ?5)",
        params![skill_name, description, central_path_str, format!("package:{}", zip_path), central_path_str],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(skill_name)
}

/// Export all skills as individual .skill packages
#[tauri::command]
pub fn export_all_skills(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<String>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare("SELECT name FROM skills")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let names: Vec<String> = rows.flatten().collect();
    let mut exported = Vec::new();

    // Reuse the export logic directly (not through tauri command dispatch)
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let exports_dir = home.join(".omnix").join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;

    for name in &names {
        // Read skill file_path
        let file_path_str: String = match conn.query_row(
            "SELECT file_path FROM skills WHERE name = ?1",
            params![name],
            |r| r.get(0),
        ) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let base_dir = PathBuf::from(&file_path_str);
        if !base_dir.exists() { continue; }

        let zip_path = exports_dir.join(format!("{}.skill", name));
        let file = match std::fs::File::create(&zip_path) { Ok(f) => f, Err(_) => continue };
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // Write metadata
        let desc: String = conn.query_row("SELECT description FROM skills WHERE name = ?1", params![name], |r| r.get(0)).unwrap_or_default();
        let metadata = serde_json::json!({ "name": name, "description": desc, "version": "1.0" });
        if zip_writer.start_file("metadata.json", options).is_ok() {
            let _ = zip_writer.write_all(serde_json::to_string_pretty(&metadata).expect("metadata serialization should not fail").as_bytes());
        }

        // Add .md files
        if let Ok(entries) = std::fs::read_dir(&base_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "md").unwrap_or(false) {
                    let fname = p.file_name().expect("entry should have a file name").to_string_lossy().to_string();
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        if zip_writer.start_file(&fname, options).is_ok() {
                            let _ = zip_writer.write_all(content.as_bytes());
                        }
                    }
                }
            }
        }

        let _ = zip_writer.finish();
        exported.push(zip_path.to_string_lossy().to_string());
    }

    Ok(exported)
}

/// Update skill category
#[tauri::command]
pub fn update_skill_category(
    skill_name: String,
    category: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET category = ?1 WHERE name = ?2",
        params![category, skill_name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

/// List all available export packages in ~/.omnix/exports/
#[tauri::command]
pub fn list_skill_packages() -> Vec<String> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let exports_dir = home.join(".omnix").join("exports");
    if !exports_dir.exists() {
        return Vec::new();
    }

    let mut packages = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&exports_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            if ext == "skill" || ext == "zip" {
                packages.push(path.to_string_lossy().to_string());
            }
        }
    }
    packages
}

// ══════════════════════════════════════════════════
// Git Skill Source Commands (P5 — DEC-018)
// ══════════════════════════════════════════════════

/// Clone a Git repository and discover skill candidates
#[tauri::command]
pub fn clone_skill_repo(
    repo_url: String,
    branch: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<GitCloneResult, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.clone_skill_repo(&repo_url, branch.as_deref())
}

/// List skill candidates from a cached Git repo
#[tauri::command]
pub fn list_repo_skills(
    repo_url: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<GitSkillCandidate>, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.list_repo_skills(&repo_url)
}

/// Import a skill from a Git repo into OMNIX
#[tauri::command]
pub fn import_git_skill(
    repo_url: String,
    skill_name: String,
    revision: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.import_git_skill(&repo_url, &skill_name, &revision)
}

/// Check for updates on Git-sourced skills
#[tauri::command]
pub fn check_git_updates(
    db: State<'_, Arc<DbManager>>,
) -> Vec<GitUpdateCheck> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.check_git_updates()
}

/// Pull updates for a specific Git-sourced skill
#[tauri::command]
pub fn pull_and_update_skill(
    skill_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.pull_and_update_skill(&skill_name)
}

/// Clean up expired Git skill cache
#[tauri::command]
pub fn cleanup_skill_cache(
    db: State<'_, Arc<DbManager>>,
) -> Result<usize, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    engine.cleanup_skill_cache()
}
