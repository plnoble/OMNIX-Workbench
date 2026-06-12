//! Skill Sync Engine
//!
//! Orchestrates skill synchronization between the central OMNIX store
//! and individual tool directories. Handles conflict detection,
//! batch operations, and drift tracking.

use rusqlite::params;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::DbManager;
use crate::tool_adapters::{AdapterRegistry, SyncMode};
use log::warn;

// ─────────────────────────────────────────────
// Conflict Detection
// ─────────────────────────────────────────────

/// Describes what exists at a sync target before we write
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConflictInfo {
    /// The tool we're syncing to
    pub tool_id: String,
    /// Target file path
    pub target_path: String,
    /// Does the target file already exist?
    pub exists: bool,
    /// Hash of the existing target content (if exists)
    pub existing_hash: Option<String>,
    /// Hash of the source content we want to sync
    pub source_hash: String,
    /// Is the existing content identical to source? (no conflict)
    pub is_identical: bool,
}

/// Strategy for resolving sync conflicts
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum ConflictStrategy {
    /// Skip this target — don't overwrite
    Skip,
    /// Overwrite the existing file
    Overwrite,
    /// Rename existing to .bak, then write new
    Rename,
}

impl std::fmt::Display for ConflictStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictStrategy::Skip => write!(f, "skip"),
            ConflictStrategy::Overwrite => write!(f, "overwrite"),
            ConflictStrategy::Rename => write!(f, "rename"),
        }
    }
}

// ─────────────────────────────────────────────
// Sync Results
// ─────────────────────────────────────────────

/// Result of syncing one skill to one tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetailedSyncResult {
    pub skill_name: String,
    pub tool_id: String,
    pub target_path: String,
    pub success: bool,
    pub conflict: Option<ConflictInfo>,
    pub strategy_used: Option<ConflictStrategy>,
    pub error: Option<String>,
}

/// Result of a batch sync operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchSyncResult {
    pub total: usize,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub details: Vec<DetailedSyncResult>,
}

/// Drift status of a skill target
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DriftStatus {
    /// Source and target are in sync
    InSync,
    /// Source has changed since last sync (needs re-sync)
    Drifted,
    /// Target file is missing (needs re-sync)
    Missing,
    /// Target file exists but was modified externally (conflict)
    Modified,
    /// Unknown — no previous sync record
    Unknown,
}

/// Drift report for a single skill+tool combination
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DriftReport {
    pub skill_name: String,
    pub tool_id: String,
    pub status: DriftStatus,
    pub source_hash: Option<String>,
    pub target_hash: Option<String>,
    pub last_synced_hash: Option<String>,
}

// ─────────────────────────────────────────────
// Sync Engine
// ─────────────────────────────────────────────

/// Central skill synchronization engine
pub struct SyncEngine {
    db: Arc<DbManager>,
    registry: AdapterRegistry,
}

impl SyncEngine {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self {
            db,
            registry: AdapterRegistry::new(),
        }
    }

    /// Read skill content from central store
    fn read_skill_content(&self, skill_name: &str) -> Result<(String, String), String> {
        let conn = self.db.get_connection().map_err(|e| e.to_string())?;
        let file_path_str: String = conn
            .query_row(
                "SELECT file_path FROM skills WHERE name = ?1",
                params![skill_name],
                |r| r.get(0),
            )
            .map_err(|e| format!("Skill '{}' not found: {}", skill_name, e))?;

        // Try SKILL.md first, then fall back to <name>_core.md
        let base = PathBuf::from(&file_path_str);
        let skill_md = base.join("SKILL.md");
        let core_md = {
            let mut p = base.clone();
            p.set_file_name(format!("{}_core.md", skill_name));
            p
        };

        let (_path, content) = if skill_md.exists() {
            let c = fs::read_to_string(&skill_md).map_err(|e| format!("Read failed: {}", e))?;
            (skill_md.to_string_lossy().to_string(), c)
        } else if core_md.exists() {
            let c = fs::read_to_string(&core_md).map_err(|e| format!("Read failed: {}", e))?;
            (core_md.to_string_lossy().to_string(), c)
        } else {
            return Err(format!("No content file found for skill '{}'", skill_name));
        };

        let hash = compute_content_hash(&content);
        Ok((content, hash))
    }

    /// Check for conflicts before syncing
    pub fn check_conflicts(
        &self,
        skill_name: &str,
        tool_ids: &[String],
    ) -> Vec<ConflictInfo> {
        let (_content, source_hash) = match self.read_skill_content(skill_name) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let mut conflicts = Vec::new();
        for tool_id in tool_ids {
            if let Some(adapter) = self.registry.get(tool_id) {
                if let Some(base) = adapter.skill_base_path() {
                    let target_dir = base.join(skill_name);
                    let target_file = target_dir.join("SKILL.md");

                    let exists = target_file.exists();
                    let existing_hash = if exists {
                        fs::read_to_string(&target_file)
                            .ok()
                            .map(|c| compute_content_hash(&c))
                    } else {
                        None
                    };

                    let is_identical = existing_hash.as_ref() == Some(&source_hash);

                    conflicts.push(ConflictInfo {
                        tool_id: tool_id.to_string(),
                        target_path: target_file.to_string_lossy().to_string(),
                        exists,
                        existing_hash,
                        source_hash: source_hash.clone(),
                        is_identical,
                    });
                }
            }
        }
        conflicts
    }

    /// Sync one skill to one tool with conflict handling
    pub fn sync_one(
        &self,
        skill_name: &str,
        tool_id: &str,
        mode: &SyncMode,
        strategy: &ConflictStrategy,
    ) -> DetailedSyncResult {
        let adapter = match self.registry.get(tool_id) {
            Some(a) => a,
            None => return DetailedSyncResult {
                skill_name: skill_name.to_string(),
                tool_id: tool_id.to_string(),
                target_path: String::new(),
                success: false,
                conflict: None,
                strategy_used: None,
                error: Some(format!("Unknown tool: {}", tool_id)),
            },
        };

        // Read source content
        let (content, source_hash) = match self.read_skill_content(skill_name) {
            Ok(r) => r,
            Err(e) => return DetailedSyncResult {
                skill_name: skill_name.to_string(),
                tool_id: tool_id.to_string(),
                target_path: String::new(),
                success: false,
                conflict: None,
                strategy_used: None,
                error: Some(e),
            },
        };

        // Check for conflict
        let target_path = adapter.skill_base_path()
            .map(|p| p.join(skill_name).join("SKILL.md").to_string_lossy().to_string())
            .unwrap_or_default();

        let target_file = PathBuf::from(&target_path);
        let exists = target_file.exists();
        let existing_hash = if exists {
            fs::read_to_string(&target_file).ok().map(|c| compute_content_hash(&c))
        } else {
            None
        };
        let is_identical = existing_hash.as_ref() == Some(&source_hash);

        let conflict = if exists && !is_identical {
            Some(ConflictInfo {
                tool_id: tool_id.to_string(),
                target_path: target_path.clone(),
                exists,
                existing_hash,
                source_hash: source_hash.clone(),
                is_identical: false,
            })
        } else {
            None
        };

        // If identical, skip (no work needed)
        if is_identical {
            self.update_target_record(skill_name, tool_id, &target_path, mode, "synced", None);
            return DetailedSyncResult {
                skill_name: skill_name.to_string(),
                tool_id: tool_id.to_string(),
                target_path,
                success: true,
                conflict: None,
                strategy_used: Some(ConflictStrategy::Skip),
                error: None,
            };
        }

        // Handle conflict according to strategy
        if exists && !is_identical {
            match strategy {
                ConflictStrategy::Skip => {
                    self.update_target_record(skill_name, tool_id, &target_path, mode, "skipped", None);
                    return DetailedSyncResult {
                        skill_name: skill_name.to_string(),
                        tool_id: tool_id.to_string(),
                        target_path,
                        success: true, // Not an error — user chose to skip
                        conflict,
                        strategy_used: Some(ConflictStrategy::Skip),
                        error: None,
                    };
                }
                ConflictStrategy::Rename => {
                    // Backup existing file
                    let bak_path = target_file.with_extension("md.bak");
                    if let Err(e) = fs::rename(&target_file, &bak_path) {
                        return DetailedSyncResult {
                            skill_name: skill_name.to_string(),
                            tool_id: tool_id.to_string(),
                            target_path,
                            success: false,
                            conflict,
                            strategy_used: Some(ConflictStrategy::Rename),
                            error: Some(format!("Failed to backup existing: {}", e)),
                        };
                    }
                }
                ConflictStrategy::Overwrite => {
                    // Just proceed — adapter will overwrite
                }
            }
        }

        // Perform the sync
        let result = adapter.sync_skill(skill_name, &content, mode);

        // Update database
        let status = if result.success { "synced" } else { "error" };
        self.update_target_record(
            skill_name,
            tool_id,
            &result.target_path,
            mode,
            status,
            result.error.as_deref(),
        );

        // Also update content_hash in skills table
        if result.success {
            if let Ok(conn) = self.db.get_connection() {
                let _ = conn.execute(
                    "UPDATE skills SET content_hash = ?1 WHERE name = ?2",
                    params![source_hash, skill_name],
                );
            }
        }

        DetailedSyncResult {
            skill_name: skill_name.to_string(),
            tool_id: tool_id.to_string(),
            target_path: result.target_path,
            success: result.success,
            conflict,
            strategy_used: Some(strategy.clone()),
            error: result.error,
        }
    }

    /// Sync one skill to multiple tools (batch for a single skill)
    pub fn sync_one_to_many(
        &self,
        skill_name: &str,
        tool_ids: &[String],
        mode: &SyncMode,
        strategy: &ConflictStrategy,
    ) -> BatchSyncResult {
        let mut details = Vec::new();
        let mut succeeded = 0;
        let mut skipped = 0;
        let mut failed = 0;

        for tool_id in tool_ids {
            let result = self.sync_one(skill_name, tool_id, mode, strategy);
            if result.success {
                if result.strategy_used == Some(ConflictStrategy::Skip) {
                    skipped += 1;
                } else {
                    succeeded += 1;
                }
            } else {
                failed += 1;
            }
            details.push(result);
        }

        BatchSyncResult {
            total: tool_ids.len(),
            succeeded,
            skipped,
            failed,
            details,
        }
    }

    /// Batch sync: sync multiple skills to all installed tools
    pub fn sync_batch(
        &self,
        skill_names: &[String],
        mode: &SyncMode,
        strategy: &ConflictStrategy,
    ) -> Vec<BatchSyncResult> {
        let installed_tools: Vec<String> = self.registry.all()
            .iter()
            .filter(|a| a.is_installed())
            .map(|a| a.tool_id().to_string())
            .collect();

        skill_names
            .iter()
            .map(|name| self.sync_one_to_many(name, &installed_tools, mode, strategy))
            .collect()
    }

    /// Re-sync all skills that have drifted
    pub fn resync_drifted(&self, mode: &SyncMode) -> Vec<DetailedSyncResult> {
        let drifted = self.check_all_drift();
        let mut results = Vec::new();

        for report in &drifted {
            if matches!(report.status, DriftStatus::Drifted | DriftStatus::Missing) {
                let result = self.sync_one(
                    &report.skill_name,
                    &report.tool_id,
                    mode,
                    &ConflictStrategy::Overwrite,
                );
                results.push(result);
            }
        }

        results
    }

    /// Check drift for a specific skill+tool combination
    pub fn check_drift(&self, skill_name: &str, tool_id: &str) -> DriftReport {
        let source_hash = self.read_skill_content(skill_name)
            .ok()
            .map(|(_, h)| h);

        let target_hash = self.registry.get(tool_id)
            .and_then(|a| a.skill_base_path())
            .map(|p| p.join(skill_name).join("SKILL.md"))
            .filter(|p| p.exists())
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|c| compute_content_hash(&c));

        let last_synced_hash = self.db.get_connection().ok()
            .and_then(|conn| {
                conn.query_row(
                    "SELECT mode FROM skill_targets WHERE skill_id = ?1 AND tool = ?2 AND status = 'synced'",
                    params![skill_name, tool_id],
                    |r| r.get::<_, String>(0),
                ).ok()
            });

        // Determine status
        let status = match (&source_hash, &target_hash) {
            (Some(sh), Some(th)) => {
                if sh == th {
                    DriftStatus::InSync
                } else {
                    DriftStatus::Drifted
                }
            }
            (Some(_), None) => DriftStatus::Missing,
            (None, Some(_)) => DriftStatus::Modified,
            (None, None) => DriftStatus::Unknown,
        };

        DriftReport {
            skill_name: skill_name.to_string(),
            tool_id: tool_id.to_string(),
            status,
            source_hash,
            target_hash,
            last_synced_hash,
        }
    }

    /// Check drift for all synced skill+tool combinations
    pub fn check_all_drift(&self) -> Vec<DriftReport> {
        let conn = match self.db.get_connection() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stmt = match conn.prepare(
            "SELECT st.skill_id, st.tool FROM skill_targets st WHERE st.status = 'synced'"
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let mut reports = Vec::new();
        for r in rows.flatten() {
            reports.push(self.check_drift(&r.0, &r.1));
        }
        reports
    }

    /// Update skill_targets record in database
    fn update_target_record(
        &self,
        skill_name: &str,
        tool_id: &str,
        target_path: &str,
        mode: &SyncMode,
        status: &str,
        error: Option<&str>,
    ) {
        if let Ok(conn) = self.db.get_connection() {
            let id = format!("{}-{}", skill_name, tool_id);
            let _ = conn.execute(
                "INSERT OR REPLACE INTO skill_targets (id, skill_id, tool, target_path, mode, status, last_error, synced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%s','now'))",
                params![id, skill_name, tool_id, target_path, mode.to_string(), status, error.unwrap_or("")],
            );
        }
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

/// Compute FNV-1a hash over full content (non-cryptographic, for change detection)
pub fn compute_content_hash(content: &str) -> String {
    crate::hash::fnv1a_hash(content)
}

// ══════════════════════════════════════════════════
// Disk Scanner (P4 — DEC-018)
// ══════════════════════════════════════════════════

/// Classification of a discovered skill relative to the OMNIX store
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum ScanClass {
    /// Skill exists in OMNIX DB and is synced to this tool — all good
    Managed,
    /// Skill exists in tool dir but NOT in OMNIX DB — candidate for import
    Unmanaged,
    /// Skill exists in OMNIX DB and has a sync target, but tool dir version differs
    Drifted,
    /// Skill has a sync target in DB but file is missing from tool dir
    Orphaned,
}

/// A single item found by the disk scanner
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanItem {
    /// Skill name (directory name)
    pub name: String,
    /// Tool that owns this directory
    pub tool_id: String,
    /// Tool display name
    pub tool_display_name: String,
    /// Full path to the SKILL.md file
    pub path: String,
    /// Content hash of the file
    pub content_hash: String,
    /// Classification
    pub class: ScanClass,
    /// File size in bytes
    pub size_bytes: u64,
    /// First line of content (for preview)
    pub preview: String,
}

/// Complete scan report
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanReport {
    /// Total items found across all tools
    pub total_found: usize,
    /// Skills that are properly managed
    pub managed: Vec<ScanItem>,
    /// Skills not yet in OMNIX — candidates for import
    pub unmanaged: Vec<ScanItem>,
    /// Skills where tool dir version differs from OMNIX
    pub drifted: Vec<ScanItem>,
    /// Skills with DB sync record but missing file on disk
    pub orphaned: Vec<ScanItem>,
    /// Tools that were scanned
    pub tools_scanned: Vec<ScannedTool>,
}

/// Summary of a scanned tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScannedTool {
    pub tool_id: String,
    pub display_name: String,
    pub is_installed: bool,
    pub skill_count: usize,
    pub skill_base_path: String,
}

impl SyncEngine {
    /// Scan all tool directories and classify every discovered skill
    pub fn scan_disk_skills(&self) -> ScanReport {
        let mut managed = Vec::new();
        let mut unmanaged = Vec::new();
        let mut drifted = Vec::new();
        let mut orphaned = Vec::new();
        let mut tools_scanned = Vec::new();

        // Get all skills in OMNIX DB for quick lookup
        let db_skills = self.get_db_skill_names();

        // Get all sync targets from DB
        let db_targets = self.get_db_sync_targets();

        for adapter in self.registry.all() {
            let is_installed = adapter.is_installed();
            let base_path = adapter.skill_base_path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let tool_info = ScannedTool {
                tool_id: adapter.tool_id().to_string(),
                display_name: adapter.display_name().to_string(),
                is_installed,
                skill_count: 0,
                skill_base_path: base_path,
            };

            if !is_installed {
                tools_scanned.push(tool_info);
                continue;
            }

            // Scan this tool's skill directory
            let discovered = adapter.list_skills();
            let mut skill_count = 0;

            for disc in &discovered {
                skill_count += 1;
                let name = disc.name.clone();
                let tool_id = adapter.tool_id().to_string();
                let content_hash = disc.content_hash.clone();

                // Read file metadata
                let path = PathBuf::from(&disc.path);
                let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let preview = fs::read_to_string(&path)
                    .ok()
                    .and_then(|c| c.lines().next().map(|l| {
                        let l = l.trim_start_matches('#').trim();
                        l.chars().take(80).collect()
                    }))
                    .unwrap_or_default();

                // Classify
                let class = if db_skills.contains(&name) {
                    // Skill exists in DB — check sync target
                    let target_key = format!("{}-{}", name, tool_id);
                    if let Some(target_hash) = db_targets.get(&target_key) {
                        if target_hash.is_empty() || *target_hash == content_hash {
                            ScanClass::Managed
                        } else {
                            ScanClass::Drifted
                        }
                    } else {
                        // In DB but no sync target for this tool — still managed centrally
                        ScanClass::Managed
                    }
                } else {
                    // Not in DB at all — unmanaged
                    ScanClass::Unmanaged
                };

                let item = ScanItem {
                    name,
                    tool_id,
                    tool_display_name: adapter.display_name().to_string(),
                    path: disc.path.clone(),
                    content_hash,
                    class: class.clone(),
                    size_bytes,
                    preview,
                };

                match class {
                    ScanClass::Managed => managed.push(item),
                    ScanClass::Unmanaged => unmanaged.push(item),
                    ScanClass::Drifted => drifted.push(item),
                    ScanClass::Orphaned => orphaned.push(item),
                }
            }

            // Check for orphaned: DB has sync target but file missing
            for (key, _) in &db_targets {
                if key.ends_with(&format!("-{}", adapter.tool_id())) {
                    let skill_name = key.trim_end_matches(&format!("-{}", adapter.tool_id()));
                    let already_found = discovered.iter().any(|d| d.name == skill_name);
                    if !already_found && db_skills.contains(&skill_name.to_string()) {
                        orphaned.push(ScanItem {
                            name: skill_name.to_string(),
                            tool_id: adapter.tool_id().to_string(),
                            tool_display_name: adapter.display_name().to_string(),
                            path: String::new(),
                            content_hash: String::new(),
                            class: ScanClass::Orphaned,
                            size_bytes: 0,
                            preview: String::new(),
                        });
                    }
                }
            }

            tools_scanned.push(ScannedTool {
                skill_count,
                ..tool_info
            });
        }

        let total_found = managed.len() + unmanaged.len() + drifted.len() + orphaned.len();

        ScanReport {
            total_found,
            managed,
            unmanaged,
            drifted,
            orphaned,
            tools_scanned,
        }
    }

    /// Import unmanaged skills into the OMNIX database
    /// Returns the number of skills successfully imported
    pub fn import_unmanaged(&self, items: &[ScanItem]) -> Result<usize, String> {
        let conn = self.db.get_connection().map_err(|e| e.to_string())?;
        let home_dir = dirs::home_dir().ok_or("Cannot determine home directory")?;
        let mut skills_dir = home_dir.clone();
        skills_dir.push(".omnix");
        skills_dir.push("skills");

        let mut imported = 0;
        for item in items {
            if item.class != ScanClass::Unmanaged {
                continue;
            }

            // Read content from the tool's skill file
            let content = match fs::read_to_string(&item.path) {
                Ok(c) => c,
                Err(_) => continue, // Skip on error
            };

            // Create central store directory
            let mut central_dir = skills_dir.clone();
            central_dir.push(&item.name);
            if let Err(_) = fs::create_dir_all(&central_dir) {
                continue;
            }

            // Write SKILL.md and profile files to central store
            let skill_md_path = central_dir.join("SKILL.md");
            if let Err(_) = fs::write(&skill_md_path, &content) {
                continue;
            }
            let core_path = central_dir.join(format!("{}_core.md", item.name));
            let _ = fs::write(&core_path, &content);
            let min_path = central_dir.join(format!("{}_minimal.md", item.name));
            let _ = fs::write(&min_path, &content);
            let comp_path = central_dir.join(format!("{}_comprehensive.md", item.name));
            let _ = fs::write(&comp_path, &content);

            let central_path_str = central_dir.to_string_lossy().to_string();
            let content_hash = compute_content_hash(&content);

            // Extract description from first heading
            let description = content.lines()
                .find(|l| l.starts_with('#'))
                .map(|l| l.trim_start_matches('#').trim().to_string())
                .unwrap_or_else(|| format!("Imported from {}", item.tool_display_name));

            // Insert into skills table
            let result = conn.execute(
                "INSERT OR IGNORE INTO skills (name, description, file_path, profile, is_active, dependencies, source_type, source_ref, central_path, content_hash)
                 VALUES (?1, ?2, ?3, 'Core', 1, '[]', ?4, ?5, ?6, ?7)",
                params![
                    item.name,
                    description,
                    central_path_str,
                    "local",  // source_type
                    format!("tool:{}", item.tool_id),  // source_ref — where it came from
                    central_path_str,
                    content_hash,
                ],
            );

            if let Ok(_) = result {
                // Also create a sync target record
                let target_id = format!("{}-{}", item.name, item.tool_id);
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO skill_targets (id, skill_id, tool, target_path, mode, status, synced_at)
                     VALUES (?1, ?2, ?3, ?4, 'copy', 'synced', strftime('%s','now'))",
                    params![target_id, item.name, item.tool_id, item.path],
                );
                imported += 1;
            }
        }

        Ok(imported)
    }

    /// Get all skill names from the OMNIX database
    fn get_db_skill_names(&self) -> Vec<String> {
        let conn = match self.db.get_connection() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare("SELECT name FROM skills") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |r| r.get::<_, String>(0)) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.flatten().collect()
    }

    /// Get all sync targets as a HashMap of "skill_id-tool" -> content_hash
    fn get_db_sync_targets(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        let conn = match self.db.get_connection() {
            Ok(c) => c,
            Err(_) => return map,
        };
        let mut stmt = match conn.prepare(
            "SELECT skill_id, tool FROM skill_targets WHERE status = 'synced'"
        ) {
            Ok(s) => s,
            Err(_) => return map,
        };
        let rows = match stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return map,
        };
        for r in rows.flatten() {
            let key = format!("{}-{}", r.0, r.1);
            map.insert(key, String::new()); // We don't store hash in targets currently
        }
        map
    }
}

// ══════════════════════════════════════════════════
// Git Skill Source (P5 — DEC-018)
// ══════════════════════════════════════════════════

/// A skill candidate discovered in a Git repository
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitSkillCandidate {
    /// Skill name (directory name under skills/)
    pub name: String,
    /// Relative path within the repo (e.g., "skills/my-skill")
    pub relative_path: String,
    /// Full local path to the SKILL.md in the cached clone
    pub local_path: String,
    /// First line of content (preview)
    pub preview: String,
    /// Content hash
    pub content_hash: String,
    /// Whether this skill already exists in OMNIX DB
    pub already_imported: bool,
}

/// Result of cloning a Git skill repo
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitCloneResult {
    /// The URL that was cloned
    pub repo_url: String,
    /// Local path where it was cloned
    pub cache_path: String,
    /// Number of skill candidates found
    pub skill_count: usize,
    /// The resolved branch/revision
    pub revision: String,
}

/// Result of checking for updates on a Git-sourced skill
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitUpdateCheck {
    pub skill_name: String,
    pub source_ref: String,
    pub current_revision: String,
    pub latest_revision: String,
    pub has_update: bool,
}

/// Number of days before cached repos are cleaned up
const GIT_CACHE_CLEANUP_DAYS: i64 = 30;

impl SyncEngine {
    /// Clone a Git repository to the skill cache directory
    pub fn clone_skill_repo(&self, repo_url: &str, branch: Option<&str>) -> Result<GitCloneResult, String> {
        let cache_dir = self.git_cache_dir();
        std::fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {}", e))?;

        // Generate a stable directory name from the URL
        let repo_dir_name = sanitize_repo_name(repo_url);
        let target_path = cache_dir.join(&repo_dir_name);

        // If already cloned, do a git pull instead
        if target_path.join(".git").exists() {
            let _ = self.git_pull(&target_path);
        } else {
            // Shallow clone
            let mut cmd = std::process::Command::new("git");
            cmd.arg("clone").arg("--depth").arg("1");
            if let Some(b) = branch {
                cmd.arg("--branch").arg(b);
            }
            cmd.arg(repo_url).arg(&target_path);

            let output = cmd.output().map_err(|e| format!("git clone failed: {}", e))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("git clone failed: {}", stderr.trim()));
            }
        }

        // Get current revision
        let revision = self.get_git_revision(&target_path)?;

        // Scan for skills
        let candidates = self.list_repo_skills_from_path(&target_path)?;

        Ok(GitCloneResult {
            repo_url: repo_url.to_string(),
            cache_path: target_path.to_string_lossy().to_string(),
            skill_count: candidates.len(),
            revision,
        })
    }

    /// List skill candidates from a cached Git repo
    pub fn list_repo_skills(&self, repo_url: &str) -> Result<Vec<GitSkillCandidate>, String> {
        let cache_dir = self.git_cache_dir();
        let repo_dir_name = sanitize_repo_name(repo_url);
        let repo_path = cache_dir.join(&repo_dir_name);

        if !repo_path.exists() {
            return Err("Repository not found in cache. Clone it first.".to_string());
        }

        self.list_repo_skills_from_path(&repo_path)
    }

    /// Import a skill from a Git repo cache into OMNIX
    pub fn import_git_skill(&self, repo_url: &str, skill_name: &str, revision: &str) -> Result<String, String> {
        let cache_dir = self.git_cache_dir();
        let repo_dir_name = sanitize_repo_name(repo_url);
        let repo_path = cache_dir.join(&repo_dir_name);

        // Find the skill in the repo
        let skill_dir = repo_path.join("skills").join(skill_name);
        let skill_file = skill_dir.join("SKILL.md");

        if !skill_file.exists() {
            return Err(format!("Skill '{}' not found in repo cache", skill_name));
        }

        let content = std::fs::read_to_string(&skill_file)
            .map_err(|e| format!("Failed to read skill: {}", e))?;

        // Create central store
        let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
        let central_dir = home.join(".omnix").join("skills").join(skill_name);
        std::fs::create_dir_all(&central_dir).map_err(|e| e.to_string())?;

        // Write files
        std::fs::write(central_dir.join("SKILL.md"), &content).map_err(|e| e.to_string())?;
        std::fs::write(central_dir.join(format!("{}_core.md", skill_name)), &content).map_err(|e| e.to_string())?;
        std::fs::write(central_dir.join(format!("{}_minimal.md", skill_name)), &content).map_err(|e| e.to_string())?;
        std::fs::write(central_dir.join(format!("{}_comprehensive.md", skill_name)), &content).map_err(|e| e.to_string())?;

        let central_path_str = central_dir.to_string_lossy().to_string();
        let content_hash = compute_content_hash(&content);

        // Extract description from first heading
        let description = content.lines()
            .find(|l| l.starts_with('#'))
            .map(|l| l.trim_start_matches('#').trim().to_string())
            .unwrap_or_else(|| format!("Imported from Git: {}", repo_url));

        // Insert into DB with git source tracking
        let conn = self.db.get_connection().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO skills (name, description, file_path, profile, is_active, dependencies, source_type, source_ref, source_revision, central_path, content_hash)
             VALUES (?1, ?2, ?3, 'Core', 1, '[]', 'git', ?4, ?5, ?6, ?7)",
            params![skill_name, description, central_path_str, repo_url, revision, central_path_str, content_hash],
        ).map_err(|e| e.to_string())?;

        Ok(skill_name.to_string())
    }

    /// Check for updates on all Git-sourced skills
    pub fn check_git_updates(&self) -> Vec<GitUpdateCheck> {
        let conn = match self.db.get_connection() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stmt = match conn.prepare(
            "SELECT name, source_ref, source_revision FROM skills WHERE source_type = 'git'"
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let mut checks = Vec::new();
        for r in rows.flatten() {
            let (name, source_ref, current_rev) = r;

            // Try to pull latest and get new revision
            let cache_dir = self.git_cache_dir();
            let repo_dir_name = sanitize_repo_name(&source_ref);
            let repo_path = cache_dir.join(&repo_dir_name);

            let latest_rev = if repo_path.join(".git").exists() {
                let _ = self.git_pull(&repo_path);
                self.get_git_revision(&repo_path).unwrap_or_default()
            } else {
                current_rev.clone()
            };

            checks.push(GitUpdateCheck {
                skill_name: name,
                source_ref,
                current_revision: current_rev.clone(),
                latest_revision: latest_rev.clone(),
                has_update: !current_rev.is_empty() && current_rev != latest_rev,
            });
        }

        checks
    }

    /// Pull updates for a specific Git-sourced skill and re-import
    pub fn pull_and_update_skill(&self, skill_name: &str) -> Result<String, String> {
        let conn = self.db.get_connection().map_err(|e| e.to_string())?;
        let (source_ref, _source_revision): (String, String) = conn
            .query_row(
                "SELECT source_ref, source_revision FROM skills WHERE name = ?1 AND source_type = 'git'",
                params![skill_name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| format!("Skill '{}' is not a git-sourced skill: {}", skill_name, e))?;

        // Pull latest
        let cache_dir = self.git_cache_dir();
        let repo_dir_name = sanitize_repo_name(&source_ref);
        let repo_path = cache_dir.join(&repo_dir_name);

        if !repo_path.join(".git").exists() {
            return Err("Repo cache not found. Re-clone required.".to_string());
        }

        self.git_pull(&repo_path)?;
        let new_revision = self.get_git_revision(&repo_path)?;

        // Re-import
        self.import_git_skill(&source_ref, skill_name, &new_revision)
    }

    /// Clean up cached repos older than GIT_CACHE_CLEANUP_DAYS
    pub fn cleanup_skill_cache(&self) -> Result<usize, String> {
        let cache_dir = self.git_cache_dir();
        if !cache_dir.exists() {
            return Ok(0);
        }

        let mut removed = 0;
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }

                // Check modification time
                if let Ok(metadata) = std::fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        let mod_time: std::time::SystemTime = modified;
                        // Simple check: if older than cutoff, remove
                        if let Ok(duration) = mod_time.elapsed() {
                            if duration.as_secs() > (GIT_CACHE_CLEANUP_DAYS as u64) * 86400 {
                                if std::fs::remove_dir_all(&path).is_ok() {
                                    removed += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(removed)
    }

    // ── Git helpers ──

    fn git_cache_dir(&self) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".omnix").join("skill_cache")
    }

    fn git_pull(&self, repo_path: &PathBuf) -> Result<(), String> {
        let output = std::process::Command::new("git")
            .arg("-C").arg(repo_path)
            .arg("pull").arg("--ff-only")
            .output()
            .map_err(|e| format!("git pull failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Non-fatal: repo might be up to date or have conflicts
            log::warn!("[git_pull] Warning: {}", stderr.trim());
        }
        Ok(())
    }

    fn get_git_revision(&self, repo_path: &PathBuf) -> Result<String, String> {
        let output = std::process::Command::new("git")
            .arg("-C").arg(repo_path)
            .arg("rev-parse").arg("--short").arg("HEAD")
            .output()
            .map_err(|e| format!("git rev-parse failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Ok("unknown".to_string())
        }
    }

    fn list_repo_skills_from_path(&self, repo_path: &PathBuf) -> Result<Vec<GitSkillCandidate>, String> {
        let skills_dir = repo_path.join("skills");
        if !skills_dir.exists() {
            return Ok(Vec::new());
        }

        let db_skill_names = self.get_db_skill_names();
        let mut candidates = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }

                let name = entry.file_name().to_string_lossy().to_string();
                let skill_file = path.join("SKILL.md");
                if !skill_file.exists() { continue; }

                let content = std::fs::read_to_string(&skill_file).unwrap_or_default();
                let preview = content.lines()
                    .find(|l| l.starts_with('#'))
                    .map(|l| l.trim_start_matches('#').trim().chars().take(80).collect())
                    .unwrap_or_default();
                let hash = compute_content_hash(&content);

                candidates.push(GitSkillCandidate {
                    name,
                    relative_path: format!("skills/{}", entry.file_name().to_string_lossy()),
                    local_path: skill_file.to_string_lossy().to_string(),
                    preview,
                    content_hash: hash,
                    already_imported: db_skill_names.contains(
                        &entry.file_name().to_string_lossy().to_string()
                    ),
                });
            }
        }

        Ok(candidates)
    }
}

/// Generate a safe directory name from a Git URL
fn sanitize_repo_name(url: &str) -> String {
    // Remove protocol and special chars
    let cleaned = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("git@")
        .replace(':', "_")
        .replace('/', "_")
        .replace('.', "_")
        .replace('-', "_");

    // Take a reasonable length
    let result: String = cleaned.chars().take(64).collect();
    if result.is_empty() {
        "unknown_repo".to_string()
    } else {
        result
    }
}
