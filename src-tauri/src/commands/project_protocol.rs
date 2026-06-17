use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;

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

fn normalize_workspace(workspace_path: &str) -> Result<PathBuf, String> {
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
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("OMNIX Project")
                .to_string()
        })
}

fn protocol_file_templates(project_name: &str) -> Vec<(&'static str, &'static str, String, &'static str)> {
    vec![
        (
            "AGENTS.md",
            "Project agent guide",
            format!(
                "# {project_name} Agent Guide\n\n## Working Agreement\n- Read this file before changing code.\n- Record important development events under `.omx/development/`.\n- Do not overwrite user changes.\n\n## Product Direction\nThis workspace uses the OMNIX project protocol: plan, implement, verify, record, distill.\n"
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
            "# Errors And Lessons\n\nRecord bugs, root causes, fixes, and reusable lessons here.\n".to_string(),
            "Anti-failure memory source.",
        ),
        (
            ".claude/skills/omnix-project-protocol/SKILL.md",
            "Claude skill bridge",
            "# OMNIX Project Protocol\n\nUse this skill when working inside this project. Read AGENTS.md and `.omx/development/current.md`, keep the worklog updated, verify before claiming completion, and propose protocol improvements as drafts instead of silently editing global rules.\n".to_string(),
            "Optional local skill bridge for Claude-compatible tools.",
        ),
    ]
}

fn build_preview(workspace_path: &str, project_name: Option<String>) -> Result<ProtocolInitPreview, String> {
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

    for (relative, _label, content, _description) in protocol_file_templates(&preview.project_name) {
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
    if let Some((event_type, event_summary, details, _created_at)) = events
        .iter()
        .find(|(event_type, event_summary, _details, _created_at)| {
            let text = format!("{} {}", event_type.to_lowercase(), event_summary.to_lowercase());
            text.contains("bug") || text.contains("error") || text.contains("failure") || text.contains("mistake")
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
