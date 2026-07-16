//! 监督台 — one live console over every running agent session.
//!
//! Pure integration, no new machinery: sessions come from `agent_sessions`,
//! pending approvals are the newest un-superseded `approval_requested` event of
//! an `awaiting_approval` session, and the action buttons reuse the existing
//! `runtime_respond_approval` / `runtime_stop_session` commands. The frontend
//! polls this one aggregate (2s) instead of subscribing per-conversation.

use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize)]
pub struct SupervisedSession {
    pub session_id: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub agent_id: String,
    pub workspace_path: String,
    pub work_mode: String,
    pub status: String,
    pub started_at: String,
    pub last_event_at: Option<String>,
    /// Present only while the session is awaiting_approval.
    pub approval: Option<PendingApproval>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingApproval {
    pub request_id: String,
    pub approval_method: String,
    pub summary: String,
    pub requested_permissions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupervisionOverview {
    pub sessions: Vec<SupervisedSession>,
    /// Sessions that finished in the last hour — the "just happened" shelf.
    pub recent_done: Vec<SupervisedSession>,
}

/// `runtime_events` carries `request_id` as a column; method/permissions live
/// inside `metadata_json` (adapter-specific keys, so probe both spellings).
fn approval_from_event(
    request_id: Option<String>,
    metadata_json: &str,
    text: Option<String>,
) -> Option<PendingApproval> {
    let metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap_or_default();
    let request_id = request_id.filter(|r| !r.is_empty()).or_else(|| {
        metadata
            .pointer("/request_id")
            .or_else(|| metadata.pointer("/requestId"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    })?;
    Some(PendingApproval {
        request_id,
        approval_method: metadata
            .pointer("/approval_method")
            .or_else(|| metadata.pointer("/approvalMethod"))
            .and_then(|v| v.as_str())
            .unwrap_or("session")
            .to_string(),
        summary: text.unwrap_or_else(|| "Agent 请求批准一个操作".to_string()),
        requested_permissions: metadata
            .pointer("/requested_permissions")
            .or_else(|| metadata.pointer("/requestedPermissions"))
            .or_else(|| metadata.pointer("/options"))
            .cloned(),
    })
}

#[tauri::command]
pub fn supervision_overview(db: State<'_, Arc<DbManager>>) -> Result<SupervisionOverview, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.conversation_id, COALESCE(c.title, s.conversation_id),
                    s.agent_id, s.workspace_path, s.work_mode, s.status, s.created_at,
                    (SELECT MAX(created_at) FROM runtime_events e WHERE e.session_id = s.id)
             FROM agent_sessions s
             LEFT JOIN conversations c ON c.id = s.conversation_id
             WHERE s.status IN ('created','starting','running','awaiting_approval','stopping')
                OR (s.status IN ('completed','failed','cancelled')
                    AND COALESCE(s.ended_at, s.created_at) >= datetime('now', '-1 hour'))
             ORDER BY s.created_at DESC
             LIMIT 100",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String, String, String, String, String, String, Option<String>)> =
        stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();

    let mut sessions = Vec::new();
    let mut recent_done = Vec::new();
    for (session_id, conversation_id, title, agent_id, workspace, work_mode, status, started, last_event) in rows {
        // Only surface the approval payload while the session actually waits on it.
        let approval = if status == "awaiting_approval" {
            conn.query_row(
                "SELECT request_id, metadata_json, text FROM runtime_events
                 WHERE session_id = ?1 AND kind = 'approval_requested'
                 ORDER BY sequence DESC LIMIT 1",
                params![session_id],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .ok()
            .and_then(|(rid, metadata, text)| approval_from_event(rid, &metadata, text))
        } else {
            None
        };
        let item = SupervisedSession {
            session_id,
            conversation_id,
            conversation_title: title,
            agent_id,
            workspace_path: workspace,
            work_mode,
            status: status.clone(),
            started_at: started,
            last_event_at: last_event,
            approval,
        };
        if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
            recent_done.push(item);
        } else {
            sessions.push(item);
        }
    }
    Ok(SupervisionOverview { sessions, recent_done })
}

#[cfg(test)]
mod tests {
    use super::approval_from_event;

    #[test]
    fn approval_payload_variants_parse() {
        // request_id as the first-class column, ACP options in metadata.
        let meta = r#"{"approval_method":"acp","options":[{"optionId":"allow"}]}"#;
        let parsed =
            approval_from_event(Some("req-1".into()), meta, Some("写入 main.rs".into())).unwrap();
        assert_eq!(parsed.request_id, "req-1");
        assert_eq!(parsed.summary, "写入 main.rs");
        assert!(parsed.requested_permissions.is_some());

        // Column empty → request id recovered from metadata (camelCase).
        let camel = r#"{"requestId":"req-2","approvalMethod":"codex"}"#;
        let parsed = approval_from_event(None, camel, None).unwrap();
        assert_eq!(parsed.request_id, "req-2");
        assert_eq!(parsed.approval_method, "codex");

        // No request id anywhere → not actionable, skip rather than mislead.
        assert!(approval_from_event(None, "not json", None).is_none());
        assert!(approval_from_event(None, r#"{"foo":1}"#, None).is_none());
    }
}
