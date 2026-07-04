use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::input_validation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolFilePreview {
    pub path: String,
    pub label: String,
    pub exists: bool,
    pub action: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolInitPreview {
    pub workspace_path: String,
    pub project_name: String,
    pub files: Vec<ProtocolFilePreview>,
    pub will_create_count: usize,
    pub will_skip_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProtocolStatus {
    pub workspace_path: String,
    pub project_name: String,
    pub enabled: bool,
    pub initialized: bool,
    pub run_id: Option<String>,
    pub last_event_at: Option<String>,
    pub pending_actions: i64,
    pub pending_proposals: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProtocolEvent {
    pub id: String,
    pub workspace_path: String,
    pub event_type: String,
    pub summary: String,
    pub details_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolActionDraft {
    pub id: String,
    pub workspace_path: String,
    pub action_type: String,
    pub title: String,
    pub content: String,
    pub diff_json: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationRun {
    pub id: String,
    pub workspace_path: String,
    pub source_summary: String,
    pub memory_count: i64,
    pub proposal_count: i64,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionProposal {
    pub id: String,
    pub workspace_path: String,
    pub proposal_type: String,
    pub title: String,
    pub rationale: String,
    pub diff_json: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
}

fn make_id(prefix: &str) -> String {
    let nanos = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_millis() * 1_000_000);
    format!("{prefix}_{nanos}_{}", std::process::id())
}

/// Validate a free-form protocol event field: bounded length, no control chars.
fn validate_event_field(value: &str, param_name: &str) -> Result<(), String> {
    input_validation::validate_content(value, param_name)?;
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(format!(
            "{} contains invalid control characters",
            param_name
        ));
    }
    Ok(())
}

/// Validate that `details_json` (if provided) is a parseable JSON object/array, then
/// bound its length. Rejects malformed JSON so it cannot be stored verbatim and
/// later break consumers that trust its shape.
fn validate_details_json(details_json: &Option<String>) -> Result<(), String> {
    let Some(raw) = details_json else {
        return Ok(());
    };
    if raw.len() > 1_048_576 {
        return Err("details_json exceeds maximum length".to_string());
    }
    serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|_| "details_json must be valid JSON".to_string())?;
    Ok(())
}

/// Validate a project name: non-empty after trim, bounded length, no control chars
/// and no path separators (it is interpolated into generated file headers).
fn validate_project_name(project_name: &str) -> Result<(), String> {
    let trimmed = project_name.trim();
    if trimmed.is_empty() {
        return Err("project_name must not be empty".to_string());
    }
    if trimmed.len() > 256 {
        return Err("project_name exceeds maximum length".to_string());
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err("project_name contains invalid characters".to_string());
    }
    Ok(())
}

fn normalize_workspace(workspace_path: &str) -> Result<PathBuf, String> {
    // Validate before touching the filesystem - mirrors runs::create_workspace_run_core
    // so a frontend-supplied system directory (e.g. C:\Windows) is rejected here too.
    input_validation::validate_workspace_path(workspace_path, "workspace_path")?;
    let path = PathBuf::from(workspace_path);
    if !path.exists() {
        return Err(format!("Workspace does not exist: {}", workspace_path));
    }
    if !path.is_dir() {
        return Err(format!("Workspace is not a directory: {}", workspace_path));
    }
    path.canonicalize().map_err(|e| e.to_string())
}

fn default_project_name(path: &Path, project_name: Option<String>) -> String {
    project_name
        .filter(|name| !name.trim().is_empty())
        .map(|name| {
            // Validate a caller-supplied name; on failure fall back to the directory name
            // rather than propagating, since this helper is also used in read-only status.
            validate_project_name(&name)
                .map(|_| name)
                .unwrap_or_else(|_| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("OMNIX Project")
                        .to_string()
                })
        })
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("OMNIX Project")
                .to_string()
        })
}

fn protocol_file_templates(
    project_name: &str,
) -> Vec<(&'static str, &'static str, String, &'static str)> {
    vec![
        (
            "AGENTS.md",
            "Project agent guide",
            format!(
                "# {project_name} Agent Guide\n\n## Working Agreement\n- Read this file before changing code.\n- Past lessons are auto-injected by OMNIX into the `OMNIX MEMORY` block of this file — heed them; do not repeat those mistakes. (No need to look them up elsewhere.)\n- **Record discipline (this is what powers distillation — do it every task/turn):** when you hit a bug, append an entry to `.omx/development/errors.md` using the template there (symptom → root cause → fix → reusable rule); log notable choices in `decisions.md`; keep the current goal/next-step in `current.md`. Be concise but concrete (include the exact command/pattern that failed).\n- Do not overwrite user changes.\n\n## Product Direction\nThis workspace uses the OMNIX project protocol: plan, implement, verify, record, distill.\n"
            ),
            "Main instructions for coding agents in this workspace.",
        ),
        (
            ".omx/development/current.md",
            "Current development state",
            format!(
                "# Current Development\n\n- Project: {project_name}\n- Status: active\n- Objective: Capture the current development goal here.\n- Next Action: Keep this section updated before and after substantial work.\n"
            ),
            "Live handoff state for the current development session.",
        ),
        (
            ".omx/development/worklog.md",
            "Worklog",
            "# Worklog\n\n- Initialized OMNIX project protocol.\n".to_string(),
            "Chronological development log.",
        ),
        (
            ".omx/development/decisions.md",
            "Decisions",
            "# Decisions\n\nRecord product and architecture decisions here.\n".to_string(),
            "Decision record for future distillation.",
        ),
        (
            ".omx/development/errors.md",
            "Errors and lessons",
            "# Errors And Lessons\n\n> OMNIX distills entries below into reusable anti-failure memories. One entry per bug.\n> Copy the template and fill it in every time you hit (and fix) a problem.\n\n## Entry template\n```\n### <short title>\n- 危险模式/命令 (the exact pattern/command that failed):\n- 根因 (root cause):\n- 修复 (fix / correct approach):\n- 可复用规则 (reusable rule):\n- 标签 (tags, comma-separated): \n```\n".to_string(),
            "Anti-failure memory source.",
        ),
        (
            ".claude/skills/omnix-project-protocol/SKILL.md",
            "Claude skill bridge",
            "# OMNIX Project Protocol\n\nUse this skill when working inside this project. Past cross-project lessons are auto-injected into the `OMNIX MEMORY` block of AGENTS.md / CLAUDE.md — heed them. Read AGENTS.md and `.omx/development/current.md` before starting. Keep the worklog updated, record bugs+root-causes+fixes under `.omx/development/errors.md`, verify before claiming completion, and propose protocol improvements as drafts instead of silently editing global rules.\n".to_string(),
            "Optional local skill bridge for Claude-compatible tools.",
        ),
    ]
}

fn build_preview(
    workspace_path: &str,
    project_name: Option<String>,
) -> Result<ProtocolInitPreview, String> {
    let workspace = normalize_workspace(workspace_path)?;
    let project_name = default_project_name(&workspace, project_name);
    let files = protocol_file_templates(&project_name)
        .into_iter()
        .map(|(relative, label, _content, description)| {
            let path = workspace.join(relative);
            let exists = path.exists();
            ProtocolFilePreview {
                path: path.to_string_lossy().to_string(),
                label: label.to_string(),
                exists,
                action: if exists { "skip" } else { "create" }.to_string(),
                description: description.to_string(),
            }
        })
        .collect::<Vec<_>>();
    let will_create_count = files.iter().filter(|file| file.action == "create").count();
    let will_skip_count = files.iter().filter(|file| file.action == "skip").count();

    Ok(ProtocolInitPreview {
        workspace_path: workspace.to_string_lossy().to_string(),
        project_name,
        files,
        will_create_count,
        will_skip_count,
    })
}

#[tauri::command]
pub fn protocol_preview_init(
    workspace_path: String,
    project_name: Option<String>,
) -> Result<ProtocolInitPreview, String> {
    build_preview(&workspace_path, project_name)
}

#[tauri::command]
pub fn protocol_get_status(
    workspace_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<ProjectProtocolStatus, String> {
    let workspace = normalize_workspace(&workspace_path)?;
    let workspace_str = workspace.to_string_lossy().to_string();
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let row = conn
        .query_row(
            "SELECT id, project_name, enabled, initialized FROM project_protocol_runs WHERE workspace_path = ?1",
            params![workspace_str],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i32>(2)?,
                r.get::<_, i32>(3)?,
            )),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let last_event_at = conn
        .query_row(
            "SELECT MAX(created_at) FROM project_protocol_events WHERE workspace_path = ?1",
            params![workspace_str],
            |r| r.get::<_, Option<String>>(0),
        )
        .map_err(|e| e.to_string())?;
    let pending_actions = conn
        .query_row(
            "SELECT COUNT(*) FROM protocol_actions WHERE workspace_path = ?1 AND status = 'pending'",
            params![workspace_str],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?;
    let pending_proposals = conn
        .query_row(
            "SELECT COUNT(*) FROM evolution_proposals WHERE workspace_path = ?1 AND status = 'pending'",
            params![workspace_str],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?;

    let (run_id, project_name, enabled, initialized) = match row {
        Some((id, name, enabled, initialized)) => (Some(id), name, enabled != 0, initialized != 0),
        None => (
            None,
            default_project_name(&workspace, None),
            false,
            workspace.join(".omx/development/current.md").exists(),
        ),
    };

    Ok(ProjectProtocolStatus {
        workspace_path: workspace_str,
        project_name,
        enabled,
        initialized,
        run_id,
        last_event_at,
        pending_actions,
        pending_proposals,
    })
}

#[tauri::command]
pub fn protocol_init_workspace(
    workspace_path: String,
    project_name: Option<String>,
    enable: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<ProjectProtocolStatus, String> {
    let preview = build_preview(&workspace_path, project_name)?;
    let workspace = PathBuf::from(&preview.workspace_path);

    for (relative, _label, content, _description) in protocol_file_templates(&preview.project_name)
    {
        let file_path = workspace.join(relative);
        if file_path.exists() {
            continue;
        }
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&file_path, content).map_err(|e| e.to_string())?;
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = make_id("protocol_run");
    conn.execute(
        "INSERT INTO project_protocol_runs (id, workspace_path, project_name, enabled, initialized, status, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, 'active', datetime('now'))
         ON CONFLICT(workspace_path) DO UPDATE SET
            project_name = excluded.project_name,
            enabled = excluded.enabled,
            initialized = 1,
            status = 'active',
            updated_at = datetime('now')",
        params![id, preview.workspace_path, preview.project_name, if enable { 1 } else { 0 }],
    )
    .map_err(|e| e.to_string())?;

    protocol_record_event(
        preview.workspace_path.clone(),
        "protocol_initialized".to_string(),
        "Project protocol initialized or refreshed without overwriting existing files.".to_string(),
        Some(serde_json::to_string(&preview.files).unwrap_or_else(|_| "[]".to_string())),
        db.clone(),
    )?;

    protocol_get_status(preview.workspace_path, db)
}

#[tauri::command]
pub fn protocol_record_event(
    workspace_path: String,
    event_type: String,
    summary: String,
    details_json: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<ProjectProtocolEvent, String> {
    let workspace = normalize_workspace(&workspace_path)?;
    let workspace_str = workspace.to_string_lossy().to_string();
    let id = make_id("protocol_event");
    // Validate frontend-supplied event fields before persisting.
    input_validation::validate_name(&event_type, "event_type")?;
    validate_event_field(&summary, "summary")?;
    validate_details_json(&details_json)?;
    let details = details_json.unwrap_or_else(|| "{}".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO project_protocol_events (id, workspace_path, event_type, summary, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, workspace_str, event_type, summary, details],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE project_protocol_runs SET updated_at = datetime('now') WHERE workspace_path = ?1",
        params![workspace.to_string_lossy().to_string()],
    )
    .map_err(|e| e.to_string())?;

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, workspace_path, event_type, summary, details_json, created_at
         FROM project_protocol_events WHERE id = ?1",
        params![id],
        |r| {
            Ok(ProjectProtocolEvent {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                event_type: r.get(2)?,
                summary: r.get(3)?,
                details_json: r.get(4)?,
                created_at: r.get(5)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

/// Auto-record key runtime events (errors, approval requests) as project-protocol
/// events for workspaces with the protocol enabled — so "problems encountered"
/// are captured without relying solely on the agent. Called from the runtime
/// event loop in `lib.rs`; holds no DB guard across an await (the caller doesn't
/// await while this runs).
pub fn protocol_auto_record(
    db: &DbManager,
    envelope: &crate::runtime_manager::SessionEventEnvelope,
) {
    use crate::runtime::RuntimeEventKind;
    let event_type = match envelope.event.kind {
        RuntimeEventKind::Error => "error",
        RuntimeEventKind::ApprovalRequested => "approval_requested",
        _ => return,
    };
    let Ok(conn) = db.get_connection() else { return };
    // session → conversation → workspace
    let raw_workspace: Option<String> = conn
        .query_row(
            "SELECT c.workspace_path FROM agent_sessions s
             JOIN conversations c ON s.conversation_id = c.id WHERE s.id = ?1",
            params![envelope.session_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let Some(raw_workspace) = raw_workspace else { return };
    if raw_workspace.trim().is_empty() || raw_workspace == "direct" {
        return;
    }
    let workspace_str = match normalize_workspace(&raw_workspace) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(_) => return,
    };
    let enabled: i64 = conn
        .query_row(
            "SELECT enabled FROM project_protocol_runs WHERE workspace_path = ?1",
            params![workspace_str],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if enabled == 0 {
        return;
    }
    let text = envelope.event.text.clone().unwrap_or_default();
    let summary: String = if text.trim().is_empty() {
        format!("Agent {event_type}")
    } else {
        text.chars().take(200).collect()
    };
    let _ = conn.execute(
        "INSERT INTO project_protocol_events (id, workspace_path, event_type, summary, details_json)
         VALUES (?1, ?2, ?3, ?4, '{\"source\":\"auto\"}')",
        params![make_id("protocol_event"), workspace_str, event_type, summary],
    );

    // Effectiveness tracking: an error that matches an existing lesson means the
    // lesson didn't prevent the repeat — bump its repeated_count so ineffective
    // lessons surface in the evolution hub.
    if event_type == "error" {
        bump_repeated_lessons(&conn, &summary);
    }
}

/// If the error text matches an active experience memory (by its code_pattern or
/// keyword overlap), increment that memory's `repeated_count`. Best-effort, single
/// best match per error to avoid over-counting.
fn bump_repeated_lessons(conn: &rusqlite::Connection, error_text: &str) {
    let needle = error_text.to_lowercase();
    let mut stmt = match conn.prepare(
        "SELECT id, code_pattern, keywords FROM memories
         WHERE type = 'experience' AND (status = 'active' OR status IS NULL OR status = '')",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .map(|rs| rs.flatten().collect())
        .unwrap_or_default();

    let mut best: Option<String> = None;
    for (id, pattern, keywords) in &rows {
        let p = pattern.trim().to_lowercase();
        let hit_pattern = p.len() >= 4 && needle.contains(&p);
        let kw_hits = keywords
            .split(',')
            .filter(|k| {
                let k = k.trim().to_lowercase();
                k.len() >= 3 && needle.contains(&k)
            })
            .count();
        if hit_pattern || kw_hits >= 2 {
            best = Some(id.clone());
            if hit_pattern {
                break; // strongest signal
            }
        }
    }
    if let Some(id) = best {
        let _ = conn.execute(
            "UPDATE memories SET repeated_count = repeated_count + 1, last_matched_at = datetime('now') WHERE id = ?1",
            params![id],
        );
    }
}

#[tauri::command]
pub fn protocol_archive_and_distill(
    workspace_path: String,
    summary: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DistillationRun, String> {
    let workspace = normalize_workspace(&workspace_path)?;
    let workspace_str = workspace.to_string_lossy().to_string();
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT event_type, summary, details_json, created_at
             FROM project_protocol_events
             WHERE workspace_path = ?1
             ORDER BY created_at DESC
             LIMIT 50",
        )
        .map_err(|e| e.to_string())?;
    let events = stmt
        .query_map(params![workspace_str], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect::<Vec<_>>();

    let source_summary = summary.unwrap_or_else(|| {
        if events.is_empty() {
            "No protocol events were recorded before this archive.".to_string()
        } else {
            events
                .iter()
                .take(8)
                .map(|(event_type, event_summary, _details, created_at)| {
                    format!("- [{created_at}] {event_type}: {event_summary}")
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    });

    let mut memory_count = 0_i64;
    if let Some((event_type, event_summary, details, _created_at)) =
        events
            .iter()
            .find(|(event_type, event_summary, _details, _created_at)| {
                let text = format!(
                    "{} {}",
                    event_type.to_lowercase(),
                    event_summary.to_lowercase()
                );
                text.contains("bug")
                    || text.contains("error")
                    || text.contains("failure")
                    || text.contains("mistake")
            })
    {
        conn.execute(
            "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type, source, workspace_path, evidence_json, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'experience', 'project_protocol', ?6, ?7, 'candidate')",
            params![
                make_id("memory"),
                event_summary,
                event_type,
                "Review the linked protocol event and turn the fix into a reusable project rule.",
                "project-protocol,distilled,candidate",
                workspace_str,
                details,
            ],
        )
        .map_err(|e| e.to_string())?;
        memory_count += 1;
    }

    let proposal_id = make_id("evolution");
    let diff_json = serde_json::json!({
        "kind": "protocol_draft",
        "workspace_path": workspace_str,
        "summary": source_summary,
        "suggested_files": [".omx/development/errors.md", ".omx/development/decisions.md", "AGENTS.md"]
    })
    .to_string();
    conn.execute(
        "INSERT INTO evolution_proposals (id, workspace_path, proposal_type, title, rationale, diff_json, status)
         VALUES (?1, ?2, 'protocol_update', ?3, ?4, ?5, 'pending')",
        params![
            proposal_id,
            workspace_str,
            "Distill archived project lessons into protocol updates",
            "Generated from project protocol events. Review before applying to project rules or skills.",
            diff_json,
        ],
    )
    .map_err(|e| e.to_string())?;

    let action_id = make_id("protocol_action");
    conn.execute(
        "INSERT INTO protocol_actions (id, workspace_path, action_type, title, content, diff_json, status)
         VALUES (?1, ?2, 'review_distillation', ?3, ?4, '{}', 'pending')",
        params![
            action_id,
            workspace_str,
            "Review archived protocol distillation",
            source_summary,
        ],
    )
    .map_err(|e| e.to_string())?;

    let run_id = make_id("distill");
    conn.execute(
        "INSERT INTO distillation_runs (id, workspace_path, source_summary, memory_count, proposal_count, status)
         VALUES (?1, ?2, ?3, ?4, 1, 'completed')",
        params![run_id, workspace_str, source_summary, memory_count],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE project_protocol_runs SET status = 'archived', archived_at = datetime('now'), updated_at = datetime('now') WHERE workspace_path = ?1",
        params![workspace.to_string_lossy().to_string()],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        "SELECT id, workspace_path, source_summary, memory_count, proposal_count, status, created_at
         FROM distillation_runs WHERE id = ?1",
        params![run_id],
        |r| {
            Ok(DistillationRun {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                source_summary: r.get(2)?,
                memory_count: r.get(3)?,
                proposal_count: r.get(4)?,
                status: r.get(5)?,
                created_at: r.get(6)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn protocol_list_actions(
    workspace_path: String,
    status: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ProtocolActionDraft>, String> {
    let workspace = normalize_workspace(&workspace_path)?;
    let workspace_str = workspace.to_string_lossy().to_string();
    let status = status.unwrap_or_else(|| "pending".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, workspace_path, action_type, title, content, diff_json, status, created_at, applied_at
             FROM protocol_actions
             WHERE workspace_path = ?1 AND (?2 = 'all' OR status = ?2)
             ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![workspace_str, status], |r| {
            Ok(ProtocolActionDraft {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                action_type: r.get(2)?,
                title: r.get(3)?,
                content: r.get(4)?,
                diff_json: r.get(5)?,
                status: r.get(6)?,
                created_at: r.get(7)?,
                applied_at: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn protocol_apply_action(
    action_id: String,
    approved: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<ProtocolActionDraft, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let status = if approved { "approved" } else { "rejected" };
    conn.execute(
        "UPDATE protocol_actions SET status = ?1, applied_at = datetime('now') WHERE id = ?2",
        params![status, action_id],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, workspace_path, action_type, title, content, diff_json, status, created_at, applied_at
         FROM protocol_actions WHERE id = ?1",
        params![action_id],
        |r| {
            Ok(ProtocolActionDraft {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                action_type: r.get(2)?,
                title: r.get(3)?,
                content: r.get(4)?,
                diff_json: r.get(5)?,
                status: r.get(6)?,
                created_at: r.get(7)?,
                applied_at: r.get(8)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn protocol_list_evolution_proposals(
    workspace_path: String,
    status: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<EvolutionProposal>, String> {
    let workspace = normalize_workspace(&workspace_path)?;
    let workspace_str = workspace.to_string_lossy().to_string();
    let status = status.unwrap_or_else(|| "pending".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, workspace_path, proposal_type, title, rationale, diff_json, status, created_at, applied_at
             FROM evolution_proposals
             WHERE workspace_path = ?1 AND (?2 = 'all' OR status = ?2)
             ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![workspace_str, status], |r| {
            Ok(EvolutionProposal {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                proposal_type: r.get(2)?,
                title: r.get(3)?,
                rationale: r.get(4)?,
                diff_json: r.get(5)?,
                status: r.get(6)?,
                created_at: r.get(7)?,
                applied_at: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn protocol_apply_evolution_proposal(
    proposal_id: String,
    approved: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<EvolutionProposal, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let status = if approved { "approved" } else { "rejected" };
    conn.execute(
        "UPDATE evolution_proposals SET status = ?1, applied_at = datetime('now') WHERE id = ?2",
        params![status, proposal_id],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, workspace_path, proposal_type, title, rationale, diff_json, status, created_at, applied_at
         FROM evolution_proposals WHERE id = ?1",
        params![proposal_id],
        |r| {
            Ok(EvolutionProposal {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                proposal_type: r.get(2)?,
                title: r.get(3)?,
                rationale: r.get(4)?,
                diff_json: r.get(5)?,
                status: r.get(6)?,
                created_at: r.get(7)?,
                applied_at: r.get(8)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

/// List every workspace that has the project protocol initialized, with its live
/// status (enabled/initialized, last event time, pending proposal/action counts).
/// Powers the evolution hub's workspace list.
#[tauri::command]
pub fn protocol_list_runs(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ProjectProtocolStatus>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let runs: Vec<(String, String, String, i32, i32)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_path, project_name, enabled, initialized
                 FROM project_protocol_runs ORDER BY updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let collected: Vec<(String, String, String, i32, i32)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i32>(3)?,
                    r.get::<_, i32>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        collected
    };

    let mut out = Vec::new();
    for (run_id, workspace_str, project_name, enabled, initialized) in runs {
        let last_event_at = conn
            .query_row(
                "SELECT MAX(created_at) FROM project_protocol_events WHERE workspace_path = ?1",
                params![workspace_str],
                |r| r.get::<_, Option<String>>(0),
            )
            .unwrap_or(None);
        let pending_actions = conn
            .query_row(
                "SELECT COUNT(*) FROM protocol_actions WHERE workspace_path = ?1 AND status = 'pending'",
                params![workspace_str],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);
        let pending_proposals = conn
            .query_row(
                "SELECT COUNT(*) FROM evolution_proposals WHERE workspace_path = ?1 AND status = 'pending'",
                params![workspace_str],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);
        out.push(ProjectProtocolStatus {
            workspace_path: workspace_str,
            project_name,
            enabled: enabled != 0,
            initialized: initialized != 0,
            run_id: Some(run_id),
            last_event_at,
            pending_actions,
            pending_proposals,
        });
    }
    Ok(out)
}

/// Enables or disables the project protocol for a workspace without touching
/// disk files. Disabled workspaces stop auto-recording and memory injection but
/// stay in the Evolution Hub list (as "未启用"). Matches the stored path as-is.
#[tauri::command]
pub fn protocol_set_enabled(
    workspace_path: String,
    enabled: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let affected = conn
        .execute(
            "UPDATE project_protocol_runs SET enabled = ?2, updated_at = CURRENT_TIMESTAMP
             WHERE workspace_path = ?1",
            params![workspace_path, if enabled { 1 } else { 0 }],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err(format!("未找到协议工作区: {workspace_path}"));
    }
    Ok(())
}

/// Removes a workspace from the Evolution Hub: deletes its protocol run, events,
/// evolution proposals and actions from OMNIX's database. Does NOT delete any
/// files on disk (the workspace's `.omx/` records and code are left untouched).
#[tauri::command]
pub fn protocol_remove_workspace(
    workspace_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let mut conn = db.get_connection().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for table in [
        "project_protocol_events",
        "evolution_proposals",
        "protocol_actions",
        "project_protocol_runs",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE workspace_path = ?1"),
            params![workspace_path],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// List recorded protocol events for a workspace (newest first). The event viewer.
/// Uses the stored workspace path as-is (no filesystem check) so it still works if
/// the folder was moved/deleted.
#[tauri::command]
pub fn protocol_list_events(
    workspace_path: String,
    limit: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ProjectProtocolEvent>, String> {
    let limit = limit.unwrap_or(100).min(500);
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, workspace_path, event_type, summary, details_json, created_at
             FROM project_protocol_events WHERE workspace_path = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![workspace_path, limit], |r| {
            Ok(ProjectProtocolEvent {
                id: r.get(0)?,
                workspace_path: r.get(1)?,
                event_type: r.get(2)?,
                summary: r.get(3)?,
                details_json: r.get(4)?,
                created_at: r.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use std::sync::Arc;

    fn temp_db() -> Arc<DbManager> {
        let p = std::env::temp_dir().join(format!(
            "omnix_proto_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&p);
        let db = DbManager::new_runtime_test(p);
        db.get_connection()
            .unwrap()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY, incident_desc TEXT, code_pattern TEXT, remediation TEXT,
                    keywords TEXT, type TEXT DEFAULT 'experience', status TEXT DEFAULT 'active',
                    confidence REAL DEFAULT 1, seen_count INTEGER DEFAULT 0,
                    repeated_count INTEGER DEFAULT 0, last_matched_at TEXT,
                    stack_tags TEXT DEFAULT '', embedding BLOB, dimensions INTEGER DEFAULT 0,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .unwrap();
        Arc::new(db)
    }

    #[test]
    fn error_matching_bumps_repeated_count() {
        let db = temp_db();
        let conn = db.get_connection().unwrap();
        conn.execute("DELETE FROM memories", []).unwrap();
        conn.execute(
            "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type, status)
             VALUES ('m1','CORS preflight','credentials include with wildcard origin','set explicit origin','cors,fetch,credentials','experience','active')",
            [],
        )
        .unwrap();

        // An error whose text contains the lesson's code_pattern → should match.
        bump_repeated_lessons(&conn, "Request blocked: credentials include with wildcard origin not allowed");
        let n: i64 = conn
            .query_row("SELECT repeated_count FROM memories WHERE id='m1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "matching error should bump repeated_count");

        // An unrelated error → no bump.
        bump_repeated_lessons(&conn, "totally unrelated compile error E0599 method not found");
        let n2: i64 = conn
            .query_row("SELECT repeated_count FROM memories WHERE id='m1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n2, 1, "unrelated error should not bump");
    }
}
