//! Remote phone-panel surface split out of proxy.rs (pure move):
//! the `/remote` HTML page and every `/api/remote/*` handler plus their
//! payload/response structs and panel-only helpers. Auth middleware and the
//! connected-client registry stay in proxy.rs. As a child module this sees the
//! parent's private items, so `use super::*;` carries all imports.
#![allow(clippy::module_inception)]

use super::*;

/// 面板页面本身。**不再往 HTML 里塞任何凭据**——页面拿到的是一个 HttpOnly
/// Cookie（由 `guard_gateway_access` 在核销配对码时种下），脚本读不到它，
/// 截图也截不出来。
pub(super) async fn serve_remote_html(_: PanelAuthed) -> impl IntoResponse {
    axum::response::Html(include_str!("remote_dashboard.html"))
}

#[derive(Debug, Serialize)]
pub(super) struct RemoteStatus {
    api_host: String,
    target_model: String,
    active_sessions: Vec<String>,
    tasks: Vec<crate::commands::DbTask>,
    cron_tasks: Vec<CronTaskInfo>,
}

#[derive(Debug, Serialize)]
pub(super) struct CronTaskInfo {
    id: String,
    title: String,
    schedule: String,
    agent_name: String,
    is_active: bool,
    last_run: Option<String>,
}

pub(super) async fn get_remote_status(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let active_sessions = state.agent_manager.get_active_session_ids();

    let mut tasks = Vec::new();
    if let Some(session_id) = active_sessions.first() {
        if let Ok(conn) = state.db.get_connection() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, conversation_id, title, status, order_num, dependencies
                 FROM tasks WHERE conversation_id = ?1 ORDER BY order_num ASC",
            ) {
                let rows = stmt.query_map(params![session_id], |row| {
                    let deps_str: String = row.get(5)?;
                    let dependencies: Vec<String> =
                        serde_json::from_str(&deps_str).unwrap_or_default();
                    Ok(crate::commands::DbTask {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        title: row.get(2)?,
                        status: row.get(3)?,
                        order_num: row.get(4)?,
                        dependencies,
                    })
                });
                if let Ok(rows) = rows {
                    tasks = rows.flatten().collect();
                }
            }
        }
    }

    let mut cron_tasks = Vec::new();
    if let Ok(conn) = state.db.get_connection() {
        if let Ok(mut stmt) = conn
            .prepare("SELECT id, title, schedule, agent_name, is_active, last_run FROM cron_tasks")
        {
            let rows = stmt.query_map([], |row| {
                let is_active_int: i32 = row.get(4)?;
                Ok(CronTaskInfo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    schedule: row.get(2)?,
                    agent_name: row.get(3)?,
                    is_active: is_active_int != 0,
                    last_run: row.get(5)?,
                })
            });
            if let Ok(rows) = rows {
                cron_tasks = rows.flatten().collect();
            }
        }
    }

    let api_host = state
        .db
        .get_setting("api_host")
        .unwrap_or(None)
        .unwrap_or_default();
    let target_model = state
        .db
        .get_setting("target_model")
        .unwrap_or(None)
        .unwrap_or_default();

    Json(RemoteStatus {
        api_host,
        target_model,
        active_sessions,
        tasks,
        cron_tasks,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
pub(super) struct ApprovePayload {
    session_id: String,
    input: String,
}

pub(super) async fn post_remote_approve(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<ApprovePayload>,
) -> impl IntoResponse {
    match state
        .agent_manager
        .send_stdin(&body.session_id, body.input.clone())
    {
        Ok(_) => (StatusCode::OK, "Success").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("Failed: {}", e)).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct SendPayload {
    session_id: String,
    message: String,
}

/// Remotely drive an active session: deliver a free-text instruction to the
/// agent (same stdin channel the approval flow uses). Gated like the rest of the
/// remote API — see `PanelAuthed`.
pub(super) async fn post_remote_send(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<SendPayload>,
) -> impl IntoResponse {
    if body.message.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "消息为空").into_response();
    }

    match state
        .agent_manager
        .send_stdin(&body.session_id, format!("{}\n", body.message.trim_end()))
    {
        Ok(_) => (StatusCode::OK, "Success").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("Failed: {}", e)).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct CronTriggerPayload {
    task_id: String,
}

pub(super) async fn post_remote_cron_trigger(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<CronTriggerPayload>,
) -> impl IntoResponse {
    let db = Arc::clone(&state.db);
    let conn_res = db.get_connection();
    if let Ok(conn) = conn_res {
        let mut stmt = match conn.prepare(
            "SELECT id, title, agent_name, args, workspace_dir FROM cron_tasks WHERE id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "DB Error").into_response(),
        };

        let row_res = stmt.query_row(params![body.task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        });

        if let Ok((id, _title, agent_name, args_str, workspace_dir)) = row_res {
            tauri::async_runtime::handle().spawn(async move {
                let _ =
                    crate::agent::run_cron_task(db, id, agent_name, args_str, workspace_dir).await;
            });
            return (StatusCode::OK, "Task triggered").into_response();
        }
    }

    (StatusCode::BAD_REQUEST, "Task not found").into_response()
}

// ── Remote chat view + control ─────────────────────────────────────────────

pub(super) fn parse_agent_id(name: &str) -> Option<crate::runtime::AgentId> {
    match name {
        "Claude Code" | "claude_code" | "claude" => Some(crate::runtime::AgentId::ClaudeCode),
        "Codex" | "codex" => Some(crate::runtime::AgentId::Codex),
        "Gemini CLI" | "gemini_cli" | "gemini" => Some(crate::runtime::AgentId::GeminiCli),
        "Qwen Code" | "qwen_code" | "qwen" => Some(crate::runtime::AgentId::QwenCode),
        "OpenCode" | "opencode" => Some(crate::runtime::AgentId::OpenCode),
        "GitHub Copilot CLI" | "copilot_cli" | "copilot" => {
            Some(crate::runtime::AgentId::CopilotCli)
        }
        _ => None,
    }
}

#[derive(Serialize)]
pub(super) struct RemoteConversation {
    id: String,
    title: String,
    agent: String,
    workspace: String,
    running: bool,
    created_at: String,
}

pub(super) async fn get_remote_conversations(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let Ok(conn) = state.db.get_connection() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "DB").into_response();
    };
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT c.id, c.title, c.active_agent, c.workspace_path, c.created_at,
                (SELECT status FROM agent_sessions s WHERE s.conversation_id = c.id ORDER BY s.created_at DESC LIMIT 1)
         FROM conversations c WHERE c.is_archived = 0 ORDER BY c.created_at DESC LIMIT 50",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            let status: Option<String> = row.get(5)?;
            Ok(RemoteConversation {
                id: row.get(0)?,
                title: row.get(1)?,
                agent: row.get(2)?,
                workspace: row.get(3)?,
                running: status.as_deref() == Some("running"),
                created_at: row.get(4)?,
            })
        }) {
            out = rows.flatten().collect();
        }
    }
    Json(out).into_response()
}

#[derive(Serialize)]
pub(super) struct RemoteMessage {
    role: String,
    content: String,
    timestamp: String,
}

pub(super) async fn get_remote_messages(
    _: PanelAuthed,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let conversation_id = params.get("conversation_id").cloned().unwrap_or_default();
    if conversation_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "conversation_id required").into_response();
    }
    let Ok(conn) = state.db.get_connection() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "DB").into_response();
    };
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT role, content, timestamp FROM messages
         WHERE conversation_id = ?1 ORDER BY timestamp ASC, rowid ASC LIMIT 300",
    ) {
        if let Ok(rows) = stmt.query_map(params![conversation_id], |row| {
            Ok(RemoteMessage { role: row.get(0)?, content: row.get(1)?, timestamp: row.get(2)? })
        }) {
            out = rows.flatten().collect();
        }
    }
    Json(out).into_response()
}

#[derive(Deserialize)]
pub(super) struct ChatPayload {
    conversation_id: String,
    text: String,
}

pub(super) async fn post_remote_chat(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<ChatPayload>,
) -> impl IntoResponse {
    if body.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "消息为空").into_response();
    }
    // Latest runtime session for this conversation.
    let session_id: Option<String> = state
        .db
        .get_connection()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT id FROM agent_sessions WHERE conversation_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![body.conversation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
        });
    let Some(session_id) = session_id else {
        return (StatusCode::BAD_REQUEST, "该会话还没有运行过 Agent，请在电脑端先发起一次").into_response();
    };

    // Try to send; if the session isn't active, resume then retry (mirrors the desktop flow).
    let text = body.text.trim();
    let rt = &state.runtime_manager;
    if rt.send_message_with_display(&session_id, text, text, false).await.is_ok() {
        return (StatusCode::OK, "Success").into_response();
    }
    if let Err(e) = rt.resume_session(&session_id).await {
        return (StatusCode::BAD_REQUEST, format!("无法恢复会话: {}", e)).into_response();
    }
    match rt.send_message_with_display(&session_id, text, text, false).await {
        Ok(_) => (StatusCode::OK, "Success").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("发送失败: {}", e)).into_response(),
    }
}

#[derive(Serialize)]
pub(super) struct RemoteAgent {
    name: String,
    installed: bool,
}

pub(super) async fn get_remote_agents(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let agents: Vec<RemoteAgent> = [
        "Claude Code",
        "Codex",
        "Gemini CLI",
        "Qwen Code",
        "OpenCode",
        "GitHub Copilot CLI",
    ]
    .iter()
    .map(|name| RemoteAgent {
        name: name.to_string(),
        installed: state.agent_manager.find_agent_path(name).is_some(),
    })
    .collect();
    Json(agents).into_response()
}

pub(super) async fn get_remote_workspaces(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let Ok(conn) = state.db.get_connection() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "DB").into_response();
    };
    let mut out: Vec<String> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT workspace_path FROM conversations
         WHERE workspace_path != '' AND workspace_path != 'direct'
         ORDER BY created_at DESC LIMIT 20",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            out = rows.flatten().collect();
        }
    }
    Json(out).into_response()
}

#[derive(Deserialize)]
pub(super) struct NewPayload {
    agent: String,
    workspace: Option<String>,
}

pub(super) async fn post_remote_new(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<NewPayload>,
) -> impl IntoResponse {
    let Some(agent) = parse_agent_id(&body.agent) else {
        return (StatusCode::BAD_REQUEST, "不支持的 Agent").into_response();
    };
    let workspace = body.workspace.clone().unwrap_or_else(|| "direct".to_string());
    let conversation_id = format!("conv_remote_{}", chrono::Utc::now().timestamp_micros());
    let title = format!("📱 远程 · {}", agent.display_name());

    // Create the conversation row first (the runtime persists messages against it).
    if let Ok(conn) = state.db.get_connection() {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
            params![conversation_id, title, workspace, agent.display_name()],
        );
    }

    match crate::commands::remote_start_session(
        &state.db,
        &state.agent_manager,
        &state.runtime_manager,
        agent,
        workspace,
        conversation_id.clone(),
    )
    .await
    {
        Ok(_) => Json(serde_json::json!({ "conversation_id": conversation_id })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("启动失败: {}", e)).into_response(),
    }
}

/// Most recent runtime session id for a conversation (or None).
pub(super) fn latest_session_for(db: &DbManager, conversation_id: &str) -> Option<String> {
    db.get_connection().ok().and_then(|conn| {
        conn.query_row(
            "SELECT id FROM agent_sessions WHERE conversation_id = ?1 ORDER BY created_at DESC LIMIT 1",
            params![conversation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
    })
}

#[derive(Serialize)]
pub(super) struct PendingApproval {
    pending: bool,
    request_id: String,
    title: String,
}

/// Whether the conversation's session is awaiting an approval the phone can answer.
pub(super) async fn get_remote_pending(
    _: PanelAuthed,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let none = || Json(PendingApproval { pending: false, request_id: String::new(), title: String::new() });
    let conversation_id = params.get("conversation_id").cloned().unwrap_or_default();
    let Some(session_id) = latest_session_for(&state.db, &conversation_id) else {
        return none().into_response();
    };
    let Ok(conn) = state.db.get_connection() else {
        return none().into_response();
    };
    let status: String = conn
        .query_row("SELECT status FROM agent_sessions WHERE id = ?1", params![session_id], |r| r.get(0))
        .unwrap_or_default();
    if status != "awaiting_approval" {
        return none().into_response();
    }
    let pending = conn
        .query_row(
            "SELECT request_id, text FROM runtime_events
             WHERE session_id = ?1 AND kind = 'approval_requested'
             ORDER BY sequence DESC LIMIT 1",
            params![session_id],
            |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get::<_, Option<String>>(1)?.unwrap_or_default())),
        )
        .ok();
    match pending {
        Some((request_id, title)) if !request_id.is_empty() => {
            Json(PendingApproval { pending: true, request_id, title }).into_response()
        }
        _ => none().into_response(),
    }
}

#[derive(Deserialize)]
pub(super) struct RespondPayload {
    conversation_id: String,
    approved: bool,
}

/// Approve/deny the pending approval from the phone (Codex sessions only —
/// Claude Code structured approval回传 is not yet supported).
pub(super) async fn post_remote_respond(
    _: PanelAuthed,
    State(state): State<Arc<ProxyState>>,
    Json(body): Json<RespondPayload>,
) -> impl IntoResponse {
    let Some(session_id) = latest_session_for(&state.db, &body.conversation_id) else {
        return (StatusCode::BAD_REQUEST, "无运行中的会话").into_response();
    };
    // Latest approval request + its method/permissions metadata.
    let row = state.db.get_connection().ok().and_then(|conn| {
        conn.query_row(
            "SELECT request_id, metadata_json FROM runtime_events
             WHERE session_id = ?1 AND kind = 'approval_requested'
             ORDER BY sequence DESC LIMIT 1",
            params![session_id],
            |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get::<_, Option<String>>(1)?.unwrap_or_default())),
        )
        .ok()
    });
    let Some((request_id, metadata_json)) = row else {
        return (StatusCode::BAD_REQUEST, "没有待处理的审批").into_response();
    };
    if request_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "没有待处理的审批").into_response();
    }
    let meta: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or(serde_json::Value::Null);
    let method = meta.get("method").and_then(|v| v.as_str()).unwrap_or("item/commandExecution/requestApproval");
    let permissions = meta.get("params").and_then(|p| p.get("permissions")).cloned();

    match state
        .runtime_manager
        .respond_approval(&session_id, &request_id, body.approved, false, method, permissions)
        .await
    {
        Ok(_) => (StatusCode::OK, "Success").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("审批失败: {}", e)).into_response(),
    }
}

// --- Dynamic Capability Routing Helpers ---

