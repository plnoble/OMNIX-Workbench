//! 技能自动更新 — keep the central store in step with skills whose *source*
//! keeps moving (e.g. officecli rewrites its skill files on every version bump).
//!
//! Three-way compare per skill, all hashes via `sync_engine::compute_content_hash`
//! (FNV-1a of SKILL.md, the store-wide convention):
//!
//! ```text
//! base    = skills.content_hash        (central content at last sync point)
//! central = hash(central SKILL.md now) (user may have edited/改造'd it)
//! source  = scan item's hash           (tool dir copy, may have been refreshed)
//! ```
//!
//! - source == central            → in sync (self-heal `base` if stale)
//! - source ≠ central == base     → clean update: backup central, pull source in,
//!                                  stamp `content_updated_at`
//! - source ≠ central ≠ base      → conflict: both sides changed — never guess,
//!                                  report and let the user pick
//!
//! Governance: an update to an **official** skill does not silently change what
//! the gateway injects *unvetted* — the pool keeps injecting (the vetted skill
//! updating itself is the normal case), but the skill is flagged 「更新待复审」
//! (`content_updated_at > reviewed_at`) with one-click re-review in the panel,
//! and the previous content is backed up for rollback.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;
use crate::sync_engine::{compute_content_hash, ScanItem, SyncEngine};

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdated {
    pub name: String,
    pub from_tool: String,
    pub backup_dir: String,
    /// The skill is official + reviewed, so the refresh needs a human look.
    pub needs_re_review: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillConflict {
    pub name: String,
    pub source_path: String,
    pub from_tool: String,
    /// Why this needs a human: "both-edited" or an injection-scan verdict.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateReport {
    pub checked: usize,
    pub updated: Vec<SkillUpdated>,
    pub conflicts: Vec<SkillConflict>,
    pub errors: Vec<String>,
}

fn central_skill_md(central_dir: &str) -> PathBuf {
    Path::new(central_dir).join("SKILL.md")
}

fn mtime(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::UNIX_EPOCH)
}

/// Newest differing source copy for one skill (multiple tools may carry it) —
/// but only sources **newer than the central copy** count as updates. An older
/// differing source is distribution lag (central moved on; the tool dir needs
/// a 同步 push), and pulling it would silently downgrade the central store.
fn pick_candidate<'a>(items: &'a [&'a ScanItem], central_md: &Path) -> Option<&'a ScanItem> {
    let central_time = mtime(central_md);
    items
        .iter()
        .filter(|item| mtime(Path::new(&item.path)) > central_time)
        .max_by_key(|item| mtime(Path::new(&item.path)))
        .copied()
}

/// Scan tool dirs and fold source-side skill updates into the central store.
/// `apply` = false runs detection only (nothing written, no backups made).
#[tauri::command]
pub fn check_skill_updates(
    apply: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillUpdateReport, String> {
    let engine = SyncEngine::new(Arc::clone(&db));
    let scan = engine.scan_disk_skills();

    // Every DB skill with a central copy: (name, base hash, central dir, official+reviewed)
    let db_skills: Vec<(String, String, String, bool)> = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT name, COALESCE(content_hash, ''), central_path,
                        (pool = 'official' AND reviewed_at IS NOT NULL)
                 FROM skills WHERE central_path != ''",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0))
            })
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };

    let mut report = SkillUpdateReport {
        checked: db_skills.len(),
        updated: Vec::new(),
        conflicts: Vec::new(),
        errors: Vec::new(),
    };
    let central_root = crate::storage::skills_dir();

    for (name, base_hash, central_dir, is_official_reviewed) in db_skills {
        let central_md = central_skill_md(&central_dir);
        let Ok(central_content) = std::fs::read_to_string(&central_md) else {
            continue; // central copy gone — the pool's own repair flows handle that
        };
        let central_hash = compute_content_hash(&central_content);

        // Sources living inside the central store are ourselves — never "updates".
        let sources: Vec<&ScanItem> = scan
            .managed
            .iter()
            .chain(scan.drifted.iter())
            .chain(scan.unmanaged.iter())
            .filter(|item| {
                item.name == name
                    && !Path::new(&item.path).starts_with(&central_root)
                    && item.content_hash != central_hash
            })
            .collect();
        let Some(candidate) = pick_candidate(&sources, &central_md) else {
            continue;
        };

        if central_hash != base_hash && !base_hash.is_empty() {
            // User (or 改造) touched the central copy AND the source moved.
            report.conflicts.push(SkillConflict {
                name,
                source_path: candidate.path.clone(),
                from_tool: candidate.tool_display_name.clone(),
                reason: "中央副本也被改过".into(),
            });
            continue;
        }

        // 安全门：自动拉取会替换正式池可能正在注入的内容——源被投毒就等于给
        // 所有 agent 下毒。高危样式不自动收编，转人工裁决。
        if let Ok(source_content) = std::fs::read_to_string(&candidate.path) {
            let scan = crate::prompt_guard::scan_for_injection(&source_content);
            if scan.should_block || scan.risk_level == "high" || scan.risk_level == "critical" {
                report.conflicts.push(SkillConflict {
                    name,
                    source_path: candidate.path.clone(),
                    from_tool: candidate.tool_display_name.clone(),
                    reason: format!(
                        "源内容命中注入样式（{}）：{}",
                        scan.risk_level,
                        scan.detected_patterns
                            .first()
                            .map(|p| p.pattern_name.clone())
                            .unwrap_or_default()
                    ),
                });
                continue;
            }
        }

        if !apply {
            report.updated.push(SkillUpdated {
                name,
                from_tool: candidate.tool_display_name.clone(),
                backup_dir: String::new(),
                needs_re_review: is_official_reviewed,
            });
            continue;
        }

        // Clean pull: backup central dir, then copy the source skill dir over it.
        let backup = crate::storage::backups_dir().join("skill_updates").join(format!(
            "{}_{}",
            name,
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        ));
        if let Err(e) = crate::storage::copy_dir_recursive(Path::new(&central_dir), &backup) {
            report.errors.push(format!("{name}: 备份失败，跳过更新（{e}）"));
            continue;
        }
        let source_dir = match Path::new(&candidate.path).parent() {
            Some(dir) => dir.to_path_buf(),
            None => continue,
        };
        if let Err(e) = crate::storage::copy_dir_recursive(&source_dir, Path::new(&central_dir)) {
            report.errors.push(format!("{name}: 更新写入失败（{e}）"));
            continue;
        }
        if let Ok(conn) = db.get_connection() {
            let _ = conn.execute(
                "UPDATE skills SET content_hash = ?1,
                        content_updated_at = CURRENT_TIMESTAMP,
                        updated_at = CURRENT_TIMESTAMP
                 WHERE name = ?2",
                params![candidate.content_hash, name],
            );
        }
        report.updated.push(SkillUpdated {
            name,
            from_tool: candidate.tool_display_name.clone(),
            backup_dir: backup.to_string_lossy().to_string(),
            needs_re_review: is_official_reviewed,
        });
    }
    Ok(report)
}

/// Resolve one conflict the user has decided on: `take_source` pulls the source
/// copy in (backing up central first); otherwise the central copy is kept and
/// re-stamped as the new base so the conflict stops re-reporting.
#[tauri::command]
pub fn resolve_skill_conflict(
    name: String,
    source_path: String,
    take_source: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let central_dir: String = conn
        .query_row(
            "SELECT central_path FROM skills WHERE name = ?1 AND central_path != ''",
            params![name],
            |r| r.get(0),
        )
        .map_err(|_| format!("技能不存在或没有中央副本: {name}"))?;

    if take_source {
        let source_md = PathBuf::from(&source_path);
        if !source_md.is_file() {
            return Err(format!("源文件已不存在: {source_path}"));
        }
        let backup = crate::storage::backups_dir().join("skill_updates").join(format!(
            "{}_{}",
            name,
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        ));
        crate::storage::copy_dir_recursive(Path::new(&central_dir), &backup)
            .map_err(|e| format!("备份失败: {e}"))?;
        let source_dir = source_md.parent().ok_or("源路径无父目录")?;
        crate::storage::copy_dir_recursive(source_dir, Path::new(&central_dir))
            .map_err(|e| format!("写入失败: {e}"))?;
    }
    // Either way the current central content becomes the new base.
    let central_content = std::fs::read_to_string(central_skill_md(&central_dir))
        .map_err(|e| format!("读中央副本失败: {e}"))?;
    conn.execute(
        "UPDATE skills SET content_hash = ?1,
                content_updated_at = CASE WHEN ?3 THEN CURRENT_TIMESTAMP ELSE content_updated_at END,
                updated_at = CURRENT_TIMESTAMP
         WHERE name = ?2",
        params![compute_content_hash(&central_content), name, take_source],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
