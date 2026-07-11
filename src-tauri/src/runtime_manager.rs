use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::proc::NoWindow;
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex as AsyncMutex, RwLock};

use serde_json::Value;

use crate::db::DbManager;
use crate::runtime::{
    agent_definition, build_branch_seed_context, build_claude_user_message,
    build_codex_approval_response, build_conversation_handoff_context, build_codex_initialize_request,
    build_codex_thread_resume_request,
    build_codex_thread_start_request, build_codex_turn_start_request, build_goal_reminder,
    build_launch_spec, conversation_has_no_messages, conversation_parent_id,
    build_resume_launch_spec, create_agent_session_record, get_active_goal_objective,
    get_agent_session_record,
    list_runtime_events, parse_claude_event, parse_codex_message, record_runtime_event,
    record_user_message, update_agent_session_status, AdapterKind, AgentId, AgentSessionConfig,
    AgentSessionRecord, AgentSessionStatus, ImageAttachment, PermissionPolicy, RuntimeEvent,
    RuntimeEventKind, WorkMode,
};
use crate::runtime_acp::{
    build_acp_cancel_notification, build_acp_error_response, build_acp_initialize_request,
    build_acp_new_session_request, build_acp_permission_response, build_acp_prompt_request,
    build_acp_read_file_response, build_acp_set_config_option_request,
    build_acp_write_file_response, classify_acp_message, request_id_from_string,
    resolve_workspace_path, select_permission_option, AcpInbound, JSONRPC_INTERNAL_ERROR,
    JSONRPC_METHOD_NOT_FOUND,
};

struct ActiveSession {
    config: AgentSessionConfig,
    stdin: AsyncMutex<ChildStdin>,
    child: Mutex<Child>,
    external_session_id: RwLock<Option<String>>,
    next_request_id: AtomicU64,
    /// Accumulates ACP `agent_message_chunk` text for the in-flight turn. ACP only
    /// streams deltas, so the manager consolidates them into a single
    /// AssistantMessage (persisted to `messages`) when the turn completes.
    acp_assistant_buffer: AsyncMutex<String>,
    /// The `configOptions` id (typically "model") the ACP agent exposes for
    /// model selection, captured from the `session/new` response. `None` for
    /// agents that don't expose one (their model is fixed to the agent default).
    acp_model_config_id: RwLock<Option<String>>,
    /// Whether the live ACP stream is currently inside a reasoning block. Drives
    /// the `<think>`/`</think>` transition markers so thoughts render collapsed
    /// in the chat bubble instead of blending into the reply text.
    acp_in_thought: AtomicBool,
    /// Options of ACP permission requests awaiting a user decision, keyed by the
    /// JSON-RPC request id string. Lets `respond_approval` map an approve/reject
    /// back to an option id without scanning the whole event table.
    acp_pending_approvals: AsyncMutex<HashMap<String, Value>>,
    /// Whether the ACP agent declared `promptCapabilities.image` at initialize —
    /// gates image attachments per-agent (gemini/opencode declare it).
    acp_supports_images: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEventEnvelope {
    pub session_id: String,
    pub event: RuntimeEvent,
}

/// Everything that makes up one outgoing user turn (see `send_user_message`).
pub struct OutgoingUserMessage<'a> {
    pub prompt: &'a str,
    pub display_text: &'a str,
    pub with_handoff: bool,
    pub images: &'a [ImageAttachment],
    /// Persisted onto the transcript row (e.g. attachment file paths).
    pub metadata: serde_json::Value,
}

pub struct RuntimeManager {
    db: Arc<DbManager>,
    active: Arc<RwLock<HashMap<String, Arc<ActiveSession>>>>,
    events: broadcast::Sender<SessionEventEnvelope>,
}

impl RuntimeManager {
    pub fn new(db: Arc<DbManager>) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            db,
            active: Arc::new(RwLock::new(HashMap::new())),
            events,
        }
    }

    pub async fn start_session(
        &self,
        config: AgentSessionConfig,
    ) -> Result<AgentSessionRecord, String> {
        let session_id = format!(
            "agent_session_{}_{}",
            chrono::Utc::now().timestamp_micros(),
            std::process::id()
        );
        create_agent_session_record(&self.db, &session_id, &config)?;
        self.launch_session(&session_id, config, None).await?;
        self.get_session(&session_id)
    }

    pub async fn resume_session(&self, session_id: &str) -> Result<AgentSessionRecord, String> {
        if self.active.read().await.contains_key(session_id) {
            return Err(format!("Agent session is already running: {session_id}"));
        }
        let record = get_agent_session_record(&self.db, session_id)?;
        let external_session_id = record
            .external_session_id
            .clone()
            .ok_or_else(|| "该会话尚未获得 Agent 外部会话 ID，无法恢复".to_string())?;
        self.launch_session(session_id, record.config, Some(external_session_id))
            .await?;
        self.get_session(session_id)
    }

    async fn launch_session(
        &self,
        session_id: &str,
        config: AgentSessionConfig,
        resume_external_id: Option<String>,
    ) -> Result<(), String> {
        update_agent_session_status(&self.db, session_id, AgentSessionStatus::Starting, None)?;

        let launch = match resume_external_id.as_deref() {
            Some(external_id) => build_resume_launch_spec(&config, external_id)?,
            None => build_launch_spec(&config)?,
        };
        let mut command = runtime_command(&launch.program, &launch.args);
        command
            .current_dir(&launch.cwd)
            .envs(&launch.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = format!("无法启动 {}: {error}", config.agent.display_name());
                let _ = update_agent_session_status(
                    &self.db,
                    session_id,
                    AgentSessionStatus::Failed,
                    Some(&message),
                );
                return Err(message);
            }
        };
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Agent process did not expose stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Agent process did not expose stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Agent process did not expose stderr".to_string())?;

        let active = Arc::new(ActiveSession {
            config: config.clone(),
            stdin: AsyncMutex::new(stdin),
            child: Mutex::new(child),
            external_session_id: RwLock::new(resume_external_id.clone()),
            next_request_id: AtomicU64::new(3),
            acp_assistant_buffer: AsyncMutex::new(String::new()),
            acp_model_config_id: RwLock::new(None),
            acp_in_thought: AtomicBool::new(false),
            acp_pending_approvals: AsyncMutex::new(HashMap::new()),
            acp_supports_images: AtomicBool::new(false),
        });
        self.active
            .write()
            .await
            .insert(session_id.to_string(), Arc::clone(&active));

        spawn_output_reader(
            BufReader::new(stdout),
            session_id.to_string(),
            config.agent,
            Arc::clone(&self.db),
            Arc::clone(&active),
            Arc::clone(&self.active),
            self.events.clone(),
        );
        spawn_log_reader(
            BufReader::new(stderr),
            session_id.to_string(),
            Arc::clone(&self.db),
            self.events.clone(),
        );

        // Per-adapter startup handshake. Both the Codex app-server and ACP agents
        // hand back their session id in a JSON-RPC response, so both wait for the
        // reader to surface it before declaring the session ready — this keeps the
        // "starting" state honest and makes the first message instant. Claude Code
        // needs no handshake (its session id arrives with the first event).
        match agent_definition(config.agent).runtime_adapter {
            AdapterKind::CodexAppServer => {
                write_json_line(&active, &build_codex_initialize_request(1)).await?;
                write_json_line(
                    &active,
                    &serde_json::json!({ "jsonrpc": "2.0", "method": "initialized" }),
                )
                .await?;
                let thread_request = match resume_external_id.as_deref() {
                    Some(thread_id) => build_codex_thread_resume_request(2, thread_id),
                    None => build_codex_thread_start_request(2, &config)?,
                };
                write_json_line(&active, &thread_request).await?;

                if let Err(error) = wait_for_external_session(&active).await {
                    self.active.write().await.remove(session_id);
                    let _ = update_agent_session_status(
                        &self.db,
                        session_id,
                        AgentSessionStatus::Failed,
                        Some(&error),
                    );
                    return Err(error);
                }
            }
            AdapterKind::Acp => {
                // ACP sessions created via `session/new` are ephemeral — they live
                // in-memory inside the agent process. Once OMNIX (and thus the agent
                // child) restarts, a stored session id is no longer loadable and the
                // agent replies "Session not found". So resume for ACP means "open a
                // fresh agent session in the same workspace": the OMNIX transcript is
                // preserved and the agent re-reads its context files + the injected
                // memory block on start. We clear any stale seeded id so the reader
                // captures the new one from the `session/new` response.
                *active.external_session_id.write().await = None;
                write_json_line(&active, &build_acp_initialize_request(1)).await?;
                write_json_line(
                    &active,
                    &build_acp_new_session_request(2, &config.workspace_path),
                )
                .await?;

                if let Err(error) = wait_for_external_session(&active).await {
                    self.active.write().await.remove(session_id);
                    let _ = update_agent_session_status(
                        &self.db,
                        session_id,
                        AgentSessionStatus::Failed,
                        Some(&error),
                    );
                    return Err(error);
                }

                // If the user previously chose a model for this agent, apply it
                // before the first prompt so the turn runs on their choice rather
                // than the agent's (possibly unusable) default.
                if let Some(preferred) = acp_model_preference(&self.db, config.agent) {
                    let config_id = active.acp_model_config_id.read().await.clone();
                    let session_id_ext = active.external_session_id.read().await.clone();
                    if let (Some(config_id), Some(session_id_ext)) = (config_id, session_id_ext) {
                        let request_id = active.next_request_id.fetch_add(1, Ordering::Relaxed);
                        let request = build_acp_set_config_option_request(
                            request_id,
                            &session_id_ext,
                            &config_id,
                            &preferred,
                        );
                        let _ = write_json_line(&active, &request).await;
                    }
                }
            }
            // Claude Code needs no handshake — its session id arrives with the
            // first streamed event.
            AdapterKind::ClaudeStreamJson => {}
        }

        Ok(())
    }

    #[cfg(test)]
    pub async fn send_message(&self, session_id: &str, prompt: &str) -> Result<(), String> {
        self.send_message_with_display(session_id, prompt, prompt, false)
            .await
    }

    /// Back-compat wrapper for text-only senders (team runs, phone remote).
    pub async fn send_message_with_display(
        &self,
        session_id: &str,
        prompt: &str,
        display_text: &str,
        with_handoff: bool,
    ) -> Result<(), String> {
        self.send_user_message(
            session_id,
            OutgoingUserMessage {
                prompt,
                display_text,
                with_handoff,
                images: &[],
                metadata: serde_json::json!({}),
            },
        )
        .await
    }

    /// Sends a user message to the running agent.
    /// - `with_handoff`: the user just switched this conversation to a different
    ///   agent — the prior transcript is prepended (sent to the agent, never
    ///   shown in the bubble; only `display_text` + `metadata` are recorded).
    /// - `images`: inline attachments, translated per adapter (Claude base64
    ///   blocks / ACP image blocks, capability-gated). Codex has no known image
    ///   input on the app-server protocol yet and rejects with a clear error.
    pub async fn send_user_message(
        &self,
        session_id: &str,
        message: OutgoingUserMessage<'_>,
    ) -> Result<(), String> {
        let active = self.active_session(session_id).await?;
        let conversation_id = active.config.conversation_id.clone();
        // Compose prompt prefixes from the earlier exchange BEFORE recording this
        // turn, so they never contain the message being sent right now. Order:
        // handoff context (agent switch) → branch seed (/btw first turn) → goal.
        let mut prefixes: Vec<String> = Vec::new();
        if message.with_handoff {
            if let Some(context) = build_conversation_handoff_context(&self.db, &conversation_id) {
                prefixes.push(context);
            }
        }
        // A `/btw` branch inherits its parent's transcript on its opening turn.
        if conversation_has_no_messages(&self.db, &conversation_id) {
            if let Some(parent) = conversation_parent_id(&self.db, &conversation_id) {
                if let Some(seed) = build_branch_seed_context(&self.db, &parent) {
                    prefixes.push(seed);
                }
            }
        }
        // An active `/goal` re-injects its objective every turn.
        if let Some(objective) = get_active_goal_objective(&self.db, &conversation_id) {
            prefixes.push(build_goal_reminder(&objective));
        }
        let owned_prompt;
        let prompt: &str = if prefixes.is_empty() {
            message.prompt
        } else {
            owned_prompt = format!("{}\n\n{}", prefixes.join("\n\n"), message.prompt);
            &owned_prompt
        };
        record_user_message(&self.db, session_id, message.display_text, message.metadata)?;
        match agent_definition(active.config.agent).runtime_adapter {
            AdapterKind::ClaudeStreamJson => {
                write_json_line(&active, &build_claude_user_message(prompt, message.images)).await
            }
            AdapterKind::CodexAppServer => {
                if !message.images.is_empty() {
                    return Err("Codex 暂不支持图片输入；请改用 Claude Code 或 Gemini CLI".into());
                }
                let thread_id = wait_for_external_session(&active).await?;
                let request_id = active.next_request_id.fetch_add(1, Ordering::Relaxed);
                let request =
                    build_codex_turn_start_request(request_id, &thread_id, prompt, &active.config)?;
                write_json_line(&active, &request).await
            }
            AdapterKind::Acp => {
                if !message.images.is_empty()
                    && !active.acp_supports_images.load(Ordering::SeqCst)
                {
                    return Err(format!(
                        "{} 未声明图片输入能力，无法带图发送",
                        active.config.agent.display_name()
                    ));
                }
                let session_id_ext = wait_for_external_session(&active).await?;
                let request_id = active.next_request_id.fetch_add(1, Ordering::Relaxed);
                let request = build_acp_prompt_request(
                    request_id,
                    &session_id_ext,
                    prompt,
                    message.images,
                );
                write_json_line(&active, &request).await
            }
        }
    }

    pub async fn respond_approval(
        &self,
        session_id: &str,
        request_id: &str,
        approved: bool,
        for_session: bool,
        approval_method: &str,
        requested_permissions: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let active = self.active_session(session_id).await?;
        match agent_definition(active.config.agent).runtime_adapter {
            AdapterKind::CodexAppServer => {
                let response = build_codex_approval_response(
                    request_id,
                    approved,
                    for_session,
                    approval_method,
                    requested_permissions,
                )?;
                write_json_line(&active, &response).await?;
                update_agent_session_status(&self.db, session_id, AgentSessionStatus::Running, None)
            }
            AdapterKind::Acp => {
                // Prefer options passed by the caller, then the in-memory pending
                // map (populated when the request arrived), then — for sessions
                // whose pending state was lost — the persisted approval event.
                let options = match requested_permissions {
                    Some(value) if value.is_array() => value,
                    _ => {
                        let pending = active
                            .acp_pending_approvals
                            .lock()
                            .await
                            .remove(request_id);
                        match pending {
                            Some(options) => options,
                            None => self.find_acp_approval_options(session_id, request_id)?,
                        }
                    }
                };
                let option_id = select_permission_option(&options, approved, for_session);
                let request_id_value = request_id_from_string(request_id);
                let response = build_acp_permission_response(&request_id_value, option_id.as_deref());
                write_json_line(&active, &response).await?;
                update_agent_session_status(&self.db, session_id, AgentSessionStatus::Running, None)
            }
            AdapterKind::ClaudeStreamJson => {
                Err("该 Agent 不支持结构化审批回传；请改用计划模式".into())
            }
        }
    }

    /// Recovers the ACP permission options recorded on the pending approval event
    /// so a boolean approve/reject can be mapped back to the agent's option id.
    fn find_acp_approval_options(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<Value, String> {
        let events = list_runtime_events(&self.db, session_id)?;
        for event in events.iter().rev() {
            if event.kind == RuntimeEventKind::ApprovalRequested
                && event.request_id.as_deref() == Some(request_id)
            {
                return Ok(event
                    .metadata
                    .get("options")
                    .cloned()
                    .unwrap_or(Value::Null));
            }
        }
        Err(format!("未找到待审批请求: {request_id}"))
    }

    /// Switches the model of a running ACP session via `session/set_config_option`
    /// and remembers the choice per-agent so future sessions start on it.
    pub async fn set_session_model(&self, session_id: &str, model: &str) -> Result<(), String> {
        if model.trim().is_empty() {
            return Err("模型不能为空".into());
        }
        let active = self.active_session(session_id).await?;
        if agent_definition(active.config.agent).runtime_adapter != AdapterKind::Acp {
            return Err("仅 ACP Agent 支持在会话中切换模型".into());
        }
        let config_id = active
            .acp_model_config_id
            .read()
            .await
            .clone()
            .ok_or_else(|| "该 Agent 未提供可切换的模型选项".to_string())?;
        let session_id_ext = active
            .external_session_id
            .read()
            .await
            .clone()
            .ok_or_else(|| "会话尚未就绪".to_string())?;
        let request_id = active.next_request_id.fetch_add(1, Ordering::Relaxed);
        let request =
            build_acp_set_config_option_request(request_id, &session_id_ext, &config_id, model);
        write_json_line(&active, &request).await?;
        // Persist as this agent's preferred model for future sessions.
        let _ = self
            .db
            .set_setting(&acp_model_setting_key(active.config.agent), model);
        Ok(())
    }

    pub async fn stop_session(&self, session_id: &str) -> Result<(), String> {
        update_agent_session_status(&self.db, session_id, AgentSessionStatus::Stopping, None)?;
        let active = self.active.write().await.remove(session_id);
        if let Some(active) = active {
            // ACP defines a graceful `session/cancel`; send it best-effort before
            // killing so the agent can unwind in-flight work cleanly.
            if agent_definition(active.config.agent).runtime_adapter == AdapterKind::Acp {
                if let Some(session_id_ext) = active.external_session_id.read().await.clone() {
                    let _ =
                        write_json_line(&active, &build_acp_cancel_notification(&session_id_ext))
                            .await;
                }
            }
            let mut child = active
                .child
                .lock()
                .map_err(|_| "Agent child lock was poisoned".to_string())?;
            match child.try_wait().map_err(|error| error.to_string())? {
                Some(_) => {}
                None => child.start_kill().map_err(|error| error.to_string())?,
            }
        }
        update_agent_session_status(&self.db, session_id, AgentSessionStatus::Cancelled, None)
    }

    pub async fn complete_session(&self, session_id: &str) -> Result<(), String> {
        let active = self.active.write().await.remove(session_id);
        if let Some(active) = active {
            let mut child = active
                .child
                .lock()
                .map_err(|_| "Agent child lock was poisoned".to_string())?;
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                child.start_kill().map_err(|error| error.to_string())?;
            }
        }
        update_agent_session_status(&self.db, session_id, AgentSessionStatus::Completed, None)
    }

    pub fn get_session(&self, session_id: &str) -> Result<AgentSessionRecord, String> {
        get_agent_session_record(&self.db, session_id)
    }

    pub fn list_events(&self, session_id: &str) -> Result<Vec<RuntimeEvent>, String> {
        list_runtime_events(&self.db, session_id)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEventEnvelope> {
        self.events.subscribe()
    }

    async fn active_session(&self, session_id: &str) -> Result<Arc<ActiveSession>, String> {
        self.active
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("Agent session is not running: {session_id}"))
    }
}

fn runtime_command(program: &str, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        let is_script = std::path::Path::new(program)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            });
        if is_script {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/S", "/C", program]).args(args);
            command.no_window();
            return command;
        }
    }
    let mut command = Command::new(program);
    command.args(args);
    command.no_window();
    command
}

async fn write_json_line(
    active: &ActiveSession,
    message: &serde_json::Value,
) -> Result<(), String> {
    let mut stdin = active.stdin.lock().await;
    let mut encoded = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    stdin
        .write_all(&encoded)
        .await
        .map_err(|error| error.to_string())?;
    stdin.flush().await.map_err(|error| error.to_string())
}

/// Maximum time to wait for Codex `thread/start` to return a thread id.
///
/// `thread/start` is not instantaneous: Codex starts the MCP servers declared
/// in the user's `~/.codex/config.toml` synchronously while handling the
/// request, so the response can arrive several seconds late (and even later
/// when some MCP servers are slow or misconfigured). The previous 5 second
/// budget timed out before a healthy response on configs with MCP servers,
/// which then left the next stdin write to hit a closing pipe (os error 232).
const CODEX_THREAD_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

async fn wait_for_external_session(active: &ActiveSession) -> Result<String, String> {
    let deadline = std::time::Instant::now() + CODEX_THREAD_START_TIMEOUT;
    loop {
        if let Some(thread_id) = active.external_session_id.read().await.clone() {
            return Ok(thread_id);
        }
        // Detect a dead agent process eagerly so we surface an actionable error
        // instead of later failing on a broken pipe write. The std mutex guard
        // is dropped before the await to avoid holding a sync lock across it.
        let exited = {
            let mut child = active
                .child
                .lock()
                .map_err(|_| "Agent child lock was poisoned".to_string())?;
            child.try_wait().map_err(|error| error.to_string())?
        };
        if let Some(status) = exited {
            return Err(format!(
                "Agent 在返回会话 ID 前已退出（{status}）。请检查该 Agent 的配置（如 Codex 的 ~/.codex/config.toml、ACP Agent 的登录状态）后重试。"
            ));
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Agent 在 {} 秒内未返回会话 ID。常见原因是启动缓慢或失效的 MCP 服务、未完成登录鉴权；请检查后重试。",
                CODEX_THREAD_START_TIMEOUT.as_secs()
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn spawn_output_reader<R>(
    reader: R,
    session_id: String,
    agent: AgentId,
    db: Arc<DbManager>,
    active: Arc<ActiveSession>,
    active_map: Arc<RwLock<HashMap<String, Arc<ActiveSession>>>>,
    events: broadcast::Sender<SessionEventEnvelope>,
) where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    let adapter = agent_definition(agent).runtime_adapter;
    tauri::async_runtime::spawn(async move {
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let parsed = match adapter {
                AdapterKind::Acp => {
                    handle_acp_line(&line, &session_id, &db, &active, &events).await;
                    continue;
                }
                AdapterKind::CodexAppServer => parse_codex_message(&line),
                AdapterKind::ClaudeStreamJson => parse_claude_event(&line),
            };
            let parsed_events = parsed.unwrap_or_else(|error| {
                vec![RuntimeEvent {
                    kind: RuntimeEventKind::RawLog,
                    text: Some(line.clone()),
                    external_session_id: None,
                    external_turn_id: None,
                    item_id: None,
                    request_id: None,
                    metadata: serde_json::json!({ "parse_error": error, "stream": "stdout" }),
                }]
            });
            for event in parsed_events {
                if let Some(external_id) = event.external_session_id.clone() {
                    *active.external_session_id.write().await = Some(external_id);
                }
                persist_and_publish(&db, &session_id, &event, &events).await;
            }
        }

        // stdout EOF: the agent process exited. Deliberate stops (stop/complete/
        // failed-handshake) remove the session from the active map BEFORE the
        // process dies, so a session still registered here crashed or quit on
        // its own — without this, the UI shows "running" forever with no error.
        let was_active = active_map.write().await.remove(&session_id).is_some();
        if was_active {
            // Read the exit status without holding the std guard across an await.
            let exit_status = match active.child.lock() {
                Ok(mut child) => child.try_wait().ok().flatten().map(|s| s.to_string()),
                Err(_) => None,
            };
            let mut event = RuntimeEvent::new(
                RuntimeEventKind::Error,
                serde_json::json!({ "process_exit": exit_status.clone(), "stream": "stdout" }),
            );
            event.text = Some(format!(
                "Agent 进程意外退出{}。会话已标记为失败；再次发送消息会自动启动新会话。",
                exit_status
                    .map(|status| format!("（{status}）"))
                    .unwrap_or_default()
            ));
            persist_and_publish(&db, &session_id, &event, &events).await;
        }
    });
}

/// Handles a single line from an ACP agent. ACP is bidirectional: notifications
/// and responses become runtime events, while `fs/*` and permission requests
/// require the client to write a JSON-RPC reply back over stdin.
async fn handle_acp_line(
    line: &str,
    session_id: &str,
    db: &Arc<DbManager>,
    active: &Arc<ActiveSession>,
    events: &broadcast::Sender<SessionEventEnvelope>,
) {
    match classify_acp_message(line) {
        Ok(AcpInbound::Emit(runtime_events)) => {
            for mut event in runtime_events {
                // Capture the agent's model config id (set before external id so
                // the handshake wait observes it once the session id is visible).
                if let Some(config_id) = event
                    .metadata
                    .pointer("/acp_model_option/config_id")
                    .and_then(|value| value.as_str())
                {
                    *active.acp_model_config_id.write().await = Some(config_id.to_string());
                }
                // Capture the agent's prompt capabilities (initialize response)
                // so image attachments can be gated per-agent.
                if let Some(image) = event
                    .metadata
                    .pointer("/acp_prompt_capabilities/image")
                    .and_then(|value| value.as_bool())
                {
                    active.acp_supports_images.store(image, Ordering::SeqCst);
                }
                if let Some(external_id) = event.external_session_id.clone() {
                    *active.external_session_id.write().await = Some(external_id);
                }

                let chunk_kind = event
                    .metadata
                    .get("acp_chunk")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                if event.kind == RuntimeEventKind::AssistantDelta {
                    match chunk_kind.as_deref() {
                        // Reasoning stream: open a <think> block on the first
                        // thought chunk so the frontend renders it collapsed
                        // (thought rows are never accumulated into the reply, so
                        // the marker can live on the persisted delta safely).
                        Some("thought") => {
                            if !active.acp_in_thought.swap(true, Ordering::SeqCst) {
                                event.text = Some(format!(
                                    "<think>{}",
                                    event.text.as_deref().unwrap_or_default()
                                ));
                            }
                        }
                        // Reply text: close a dangling think block first (as a
                        // transient, publish-only delta so the marker never leaks
                        // into the consolidated persisted reply), then accumulate
                        // the untouched chunk for the turn-end AssistantMessage.
                        Some("message") => {
                            if active.acp_in_thought.swap(false, Ordering::SeqCst) {
                                publish_transient_delta(session_id, "</think>\n", events);
                            }
                            if let Some(text) = event.text.as_deref() {
                                active.acp_assistant_buffer.lock().await.push_str(text);
                            }
                        }
                        _ => {}
                    }
                }

                // Flush the accumulated assistant message before the turn-complete
                // marker so it is sequenced ahead of it in the transcript. Close a
                // think block left open by a turn that ended during reasoning.
                if event.kind == RuntimeEventKind::TurnCompleted {
                    if active.acp_in_thought.swap(false, Ordering::SeqCst) {
                        publish_transient_delta(session_id, "</think>\n", events);
                    }
                    flush_acp_assistant_message(session_id, db, active, events).await;
                }
                persist_and_publish(db, session_id, &event, events).await;
            }
        }
        Ok(AcpInbound::ReadFile {
            request_id,
            path,
            line: start,
            limit,
        }) => {
            let response =
                serve_acp_read_file(&active.config.workspace_path, &request_id, &path, start, limit)
                    .await;
            let _ = write_json_line(active, &response).await;
            persist_and_publish(db, session_id, &acp_fs_audit_event("read", &path), events).await;
        }
        Ok(AcpInbound::WriteFile {
            request_id,
            path,
            content,
        }) => {
            let response =
                serve_acp_write_file(&active.config.workspace_path, &request_id, &path, &content)
                    .await;
            let _ = write_json_line(active, &response).await;
            persist_and_publish(db, session_id, &acp_fs_audit_event("write", &path), events).await;
        }
        Ok(AcpInbound::Permission {
            request_id,
            options,
            event,
        }) => {
            handle_acp_permission(request_id, options, event, session_id, db, active, events).await;
        }
        Ok(AcpInbound::UnsupportedRequest { request_id, method }) => {
            let response = build_acp_error_response(
                &request_id,
                JSONRPC_METHOD_NOT_FOUND,
                &format!("OMNIX 不支持该 Agent 请求: {method}"),
            );
            let _ = write_json_line(active, &response).await;
        }
        Err(error) => {
            let event = RuntimeEvent {
                kind: RuntimeEventKind::RawLog,
                text: Some(line.to_string()),
                external_session_id: None,
                external_turn_id: None,
                item_id: None,
                request_id: None,
                metadata: serde_json::json!({ "parse_error": error, "stream": "stdout" }),
            };
            persist_and_publish(db, session_id, &event, events).await;
        }
    }
}

/// Applies the session's permission policy to an ACP permission request:
/// plan mode auto-rejects, confirmed full access auto-approves, and everything
/// else surfaces the request for the user to decide via `respond_approval`.
async fn handle_acp_permission(
    request_id: Value,
    options: Value,
    event: RuntimeEvent,
    session_id: &str,
    db: &Arc<DbManager>,
    active: &Arc<ActiveSession>,
    events: &broadcast::Sender<SessionEventEnvelope>,
) {
    let auto_decision = match active.config.work_mode {
        WorkMode::Plan => Some(false),
        WorkMode::Direct => {
            if matches!(
                active.config.permission,
                PermissionPolicy::FullAccess { confirmed: true }
            ) {
                Some(true)
            } else {
                None
            }
        }
    };

    match auto_decision {
        Some(approved) => {
            let option_id = select_permission_option(&options, approved, approved);
            let response = build_acp_permission_response(&request_id, option_id.as_deref());
            let _ = write_json_line(active, &response).await;
            let mut audit = event;
            audit.kind = RuntimeEventKind::RawLog;
            audit.metadata = serde_json::json!({
                "adapter": "acp",
                "auto_decision": if approved { "approved" } else { "rejected" },
                "reason": if approved { "full_access" } else { "plan_mode" },
            });
            persist_and_publish(db, session_id, &audit, events).await;
        }
        None => {
            // Remember the offered options so respond_approval can map the
            // user's boolean decision back to an option id without a DB scan.
            if let Some(request_key) = event.request_id.clone() {
                active
                    .acp_pending_approvals
                    .lock()
                    .await
                    .insert(request_key, options);
            }
            let _ = update_agent_session_status(
                db,
                session_id,
                AgentSessionStatus::AwaitingApproval,
                None,
            );
            persist_and_publish(db, session_id, &event, events).await;
        }
    }
}

/// Reads a file the ACP agent requested, constrained to the session workspace.
async fn serve_acp_read_file(
    workspace_root: &str,
    request_id: &Value,
    path: &str,
    start: Option<u32>,
    limit: Option<u32>,
) -> Value {
    match resolve_workspace_path(workspace_root, path) {
        Ok(resolved) => match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => {
                build_acp_read_file_response(request_id, &apply_line_window(&content, start, limit))
            }
            Err(error) => build_acp_error_response(
                request_id,
                JSONRPC_INTERNAL_ERROR,
                &format!("读取文件失败: {error}"),
            ),
        },
        Err(error) => build_acp_error_response(request_id, JSONRPC_INTERNAL_ERROR, &error),
    }
}

/// Writes a file the ACP agent requested, constrained to the session workspace.
async fn serve_acp_write_file(
    workspace_root: &str,
    request_id: &Value,
    path: &str,
    content: &str,
) -> Value {
    match resolve_workspace_path(workspace_root, path) {
        Ok(resolved) => {
            if let Some(parent) = resolved.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            match tokio::fs::write(&resolved, content).await {
                Ok(()) => build_acp_write_file_response(request_id),
                Err(error) => build_acp_error_response(
                    request_id,
                    JSONRPC_INTERNAL_ERROR,
                    &format!("写入文件失败: {error}"),
                ),
            }
        }
        Err(error) => build_acp_error_response(request_id, JSONRPC_INTERNAL_ERROR, &error),
    }
}

/// Applies ACP's optional 1-based `line` start and `limit` window to file text.
fn apply_line_window(content: &str, start: Option<u32>, limit: Option<u32>) -> String {
    if start.is_none() && limit.is_none() {
        return content.to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    let start_index = start.map(|value| value.saturating_sub(1) as usize).unwrap_or(0);
    if start_index >= lines.len() {
        return String::new();
    }
    let end_index = match limit {
        Some(limit) => start_index.saturating_add(limit as usize).min(lines.len()),
        None => lines.len(),
    };
    lines[start_index..end_index].join("\n")
}

/// Settings key holding the user's preferred model for an ACP agent. Keyed by
/// the stable wire id (not the display name, which is free to change).
pub(crate) fn acp_model_setting_key(agent: AgentId) -> String {
    format!("acp_model_{}", crate::runtime::agent_id_str(agent))
}

/// Reads the user's saved model preference for an ACP agent, if any. Falls back
/// to the legacy display-name key written by earlier builds.
pub(crate) fn acp_model_preference(db: &Arc<DbManager>, agent: AgentId) -> Option<String> {
    let read = |key: String| {
        db.get_setting(&key)
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
    };
    read(acp_model_setting_key(agent))
        .or_else(|| read(format!("acp_model_{}", agent.display_name())))
}

/// Publishes a synthetic streaming delta to live subscribers WITHOUT persisting
/// it — used for `</think>` transition markers that must reach the streaming
/// bubble but must never enter the database or the consolidated reply.
fn publish_transient_delta(
    session_id: &str,
    text: &str,
    events: &broadcast::Sender<SessionEventEnvelope>,
) {
    let mut event = RuntimeEvent::new(
        RuntimeEventKind::AssistantDelta,
        serde_json::json!({ "acp_chunk": "marker" }),
    );
    event.text = Some(text.to_string());
    let _ = events.send(SessionEventEnvelope {
        session_id: session_id.to_string(),
        event,
    });
}

/// Emits the accumulated ACP assistant text as a single `AssistantMessage` so it
/// is persisted to `messages` and survives a conversation reload. No-op when the
/// buffer is empty (e.g. a tool-only turn).
async fn flush_acp_assistant_message(
    session_id: &str,
    db: &Arc<DbManager>,
    active: &Arc<ActiveSession>,
    events: &broadcast::Sender<SessionEventEnvelope>,
) {
    let text = {
        let mut buffer = active.acp_assistant_buffer.lock().await;
        std::mem::take(&mut *buffer)
    };
    if text.trim().is_empty() {
        return;
    }
    let mut event = RuntimeEvent::new(
        RuntimeEventKind::AssistantMessage,
        serde_json::json!({ "adapter": "acp" }),
    );
    event.text = Some(text);
    persist_and_publish(db, session_id, &event, events).await;
}

/// Records that OMNIX served a filesystem request on the agent's behalf.
fn acp_fs_audit_event(operation: &str, path: &str) -> RuntimeEvent {
    RuntimeEvent {
        kind: RuntimeEventKind::RawLog,
        text: Some(format!("fs/{operation} {path}")),
        external_session_id: None,
        external_turn_id: None,
        item_id: None,
        request_id: None,
        metadata: serde_json::json!({ "adapter": "acp", "fs_operation": operation, "path": path }),
    }
}

fn spawn_log_reader<R>(
    reader: R,
    session_id: String,
    db: Arc<DbManager>,
    events: broadcast::Sender<SessionEventEnvelope>,
) where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let event = RuntimeEvent {
                kind: RuntimeEventKind::RawLog,
                text: Some(line),
                external_session_id: None,
                external_turn_id: None,
                item_id: None,
                request_id: None,
                metadata: serde_json::json!({ "stream": "stderr" }),
            };
            persist_and_publish(&db, &session_id, &event, &events).await;
        }
    });
}

async fn persist_and_publish(
    db: &Arc<DbManager>,
    session_id: &str,
    event: &RuntimeEvent,
    events: &broadcast::Sender<SessionEventEnvelope>,
) {
    let db = Arc::clone(db);
    let persisted_event = event.clone();
    let persisted_session_id = session_id.to_string();
    let persisted = tokio::task::spawn_blocking(move || {
        record_runtime_event(&db, &persisted_session_id, &persisted_event)
    })
    .await;
    if !matches!(persisted, Ok(Ok(()))) {
        log::error!("Failed to persist runtime event for session {session_id}");
        return;
    }
    let _ = events.send(SessionEventEnvelope {
        session_id: session_id.to_string(),
        event: event.clone(),
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusqlite::OptionalExtension;

    use crate::db::DbManager;
    use crate::runtime::{
        AgentId, AgentSessionConfig, AgentSessionStatus, ModelSelection, PermissionPolicy,
        RuntimeEventKind, WorkMode,
    };

    use super::RuntimeManager;

    #[cfg(windows)]
    #[tokio::test]
    async fn fake_claude_process_persists_structured_output() {
        let suffix = chrono::Utc::now().timestamp_micros();
        let db_path = std::env::temp_dir().join(format!("omnix_runtime_manager_{suffix}.sqlite"));
        let script_path = std::env::temp_dir().join(format!("omnix_fake_claude_{suffix}.cmd"));
        std::fs::write(
            &script_path,
            "@echo off\r\nset /p OMNIX_INPUT=\r\necho {\"type\":\"assistant\",\"session_id\":\"fake-claude-session\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"fake response\"}]}}\r\n",
        )
        .expect("fake Claude script");

        let db = Arc::new(DbManager::new_runtime_test(db_path.clone()));
        let conn = db.get_connection().expect("db connection");
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["conv-runtime", "Runtime", "D:/work/project", "Claude Code"],
        )
        .expect("conversation seed");
        drop(conn);

        let manager = RuntimeManager::new(Arc::clone(&db));
        let session = manager
            .start_session(AgentSessionConfig {
                conversation_id: "conv-runtime".into(),
                agent: AgentId::ClaudeCode,
                executable_path: script_path.to_string_lossy().into_owned(),
                workspace_path: std::env::temp_dir().to_string_lossy().into_owned(),
                model: ModelSelection::AgentDefault,
                permission: PermissionPolicy::AskOnRisk,
                work_mode: WorkMode::Direct,
            })
            .await
            .expect("start fake session");
        manager
            .send_message(&session.id, "run the fake task")
            .await
            .expect("send fake task");

        let mut events = Vec::new();
        for _ in 0..40 {
            events = manager.list_events(&session.id).expect("list events");
            if events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::AssistantMessage)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(events.iter().any(|event| {
            event.kind == RuntimeEventKind::AssistantMessage
                && event.text.as_deref() == Some("fake response")
        }));

        manager
            .stop_session(&session.id)
            .await
            .expect("stop session");
        let stopped = manager.get_session(&session.id).expect("reload session");
        assert_eq!(stopped.status, AgentSessionStatus::Cancelled);

        drop(manager);
        drop(db);
        let _ = std::fs::remove_file(script_path);
        let _ = std::fs::remove_file(db_path);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn fake_codex_app_server_completes_json_rpc_turn() {
        let suffix = chrono::Utc::now().timestamp_micros();
        let db_path = std::env::temp_dir().join(format!("omnix_codex_manager_{suffix}.sqlite"));
        let script_path = std::env::temp_dir().join(format!("omnix_fake_codex_{suffix}.cmd"));
        let worker_path = std::env::temp_dir().join(format!("omnix_fake_codex_{suffix}.ps1"));
        std::fs::write(
            &worker_path,
            "[Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"thread\":{\"id\":\"fake-codex-thread\"}}}')\nfor ($i = 0; $i -lt 4; $i++) { [void][Console]::In.ReadLine() }\n[Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"method\":\"item/completed\",\"params\":{\"threadId\":\"fake-codex-thread\",\"turnId\":\"turn-1\",\"item\":{\"id\":\"message-1\",\"type\":\"agentMessage\",\"text\":\"codex response\"}}}')\n",
        )
        .expect("fake Codex worker");
        std::fs::write(
            &script_path,
            format!(
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"\r\n",
                worker_path.display()
            ),
        )
        .expect("fake Codex script");

        let db = Arc::new(DbManager::new_runtime_test(db_path.clone()));
        let conn = db.get_connection().expect("db connection");
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["conv-codex-runtime", "Codex Runtime", "D:/work/project", "Codex"],
        )
        .expect("conversation seed");
        drop(conn);

        let manager = RuntimeManager::new(Arc::clone(&db));
        let session = manager
            .start_session(AgentSessionConfig {
                conversation_id: "conv-codex-runtime".into(),
                agent: AgentId::Codex,
                executable_path: script_path.to_string_lossy().into_owned(),
                workspace_path: std::env::temp_dir().to_string_lossy().into_owned(),
                model: ModelSelection::AgentDefault,
                permission: PermissionPolicy::AskOnRisk,
                work_mode: WorkMode::Direct,
            })
            .await
            .expect("start fake Codex session");
        manager
            .send_message(&session.id, "run Codex test")
            .await
            .expect("send Codex turn");

        let mut events = Vec::new();
        for _ in 0..40 {
            events = manager.list_events(&session.id).expect("list events");
            if events.iter().any(|event| {
                event.kind == RuntimeEventKind::AssistantMessage
                    && event.text.as_deref() == Some("codex response")
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            events.iter().any(|event| {
                event.kind == RuntimeEventKind::AssistantMessage
                    && event.text.as_deref() == Some("codex response")
            }),
            "events={events:#?}"
        );

        manager
            .stop_session(&session.id)
            .await
            .expect("stop Codex session");
        drop(manager);
        drop(db);
        let _ = std::fs::remove_file(script_path);
        let _ = std::fs::remove_file(worker_path);
        let _ = std::fs::remove_file(db_path);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn fake_acp_agent_streams_serves_fs_and_auto_approves() {
        let suffix = chrono::Utc::now().timestamp_micros();
        let db_path = std::env::temp_dir().join(format!("omnix_acp_manager_{suffix}.sqlite"));
        let script_path = std::env::temp_dir().join(format!("omnix_fake_acp_{suffix}.cmd"));
        let worker_path = std::env::temp_dir().join(format!("omnix_fake_acp_{suffix}.ps1"));
        let workspace = std::env::temp_dir().join(format!("omnix_acp_ws_{suffix}"));
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        let written_file = workspace.join("acp_out.txt");

        // A minimal ACP agent: answers the initialize + session/new handshake,
        // then on a prompt streams a message, asks OMNIX to write a file, asks
        // for permission (auto-approved under full access), and ends the turn.
        std::fs::write(
            &worker_path,
            "[void][Console]::In.ReadLine()\n\
             [Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}')\n\
             [void][Console]::In.ReadLine()\n\
             [Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"fake-acp-session\",\"configOptions\":[{\"id\":\"model\",\"category\":\"model\",\"currentValue\":\"m-default\",\"options\":[{\"value\":\"m-default\",\"name\":\"Default\"},{\"value\":\"m-alt\",\"name\":\"Alt\"}]}]}}')\n\
             [void][Console]::In.ReadLine()\n\
             [Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"fake-acp-session\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"acp hello\"}}}}')\n\
             [Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":\"w1\",\"method\":\"fs/write_text_file\",\"params\":{\"sessionId\":\"fake-acp-session\",\"path\":\"acp_out.txt\",\"content\":\"written-by-omnix\"}}')\n\
             [Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":\"p1\",\"method\":\"session/request_permission\",\"params\":{\"sessionId\":\"fake-acp-session\",\"toolCall\":{\"toolCallId\":\"tc1\",\"title\":\"Edit files\"},\"options\":[{\"optionId\":\"allow\",\"name\":\"Allow\",\"kind\":\"allow_always\"},{\"optionId\":\"reject\",\"name\":\"Reject\",\"kind\":\"reject_once\"}]}}')\n\
             [Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"stopReason\":\"end_turn\"}}')\n\
             Start-Sleep -Seconds 3\n",
        )
        .expect("fake ACP worker");
        std::fs::write(
            &script_path,
            format!(
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"\r\n",
                worker_path.display()
            ),
        )
        .expect("fake ACP script");

        let db = Arc::new(DbManager::new_runtime_test(db_path.clone()));
        let conn = db.get_connection().expect("db connection");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT, updated_at DATETIME);",
        )
        .expect("settings table");
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["conv-acp-runtime", "ACP Runtime", "D:/work/project", "Gemini CLI"],
        )
        .expect("conversation seed");
        drop(conn);

        let manager = RuntimeManager::new(Arc::clone(&db));
        let session = manager
            .start_session(AgentSessionConfig {
                conversation_id: "conv-acp-runtime".into(),
                agent: AgentId::GeminiCli,
                executable_path: script_path.to_string_lossy().into_owned(),
                workspace_path: workspace.to_string_lossy().into_owned(),
                model: ModelSelection::AgentDefault,
                permission: PermissionPolicy::FullAccess { confirmed: true },
                work_mode: WorkMode::Direct,
            })
            .await
            .expect("start fake ACP session");

        // The handshake must have resolved the ACP session id before readiness.
        assert_eq!(
            session.external_session_id.as_deref(),
            Some("fake-acp-session")
        );

        manager
            .send_message(&session.id, "do the ACP task")
            .await
            .expect("send ACP prompt");

        let mut events = Vec::new();
        let mut file_written = false;
        for _ in 0..60 {
            events = manager.list_events(&session.id).expect("list events");
            file_written = std::fs::read_to_string(&written_file)
                .map(|content| content == "written-by-omnix")
                .unwrap_or(false);
            let turn_done = events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::TurnCompleted);
            if file_written && turn_done {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        assert!(file_written, "OMNIX should have served fs/write_text_file");
        assert!(
            events.iter().any(|event| {
                event.kind == RuntimeEventKind::AssistantDelta
                    && event.text.as_deref() == Some("acp hello")
            }),
            "expected streamed assistant text; events={events:#?}"
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::TurnCompleted),
            "expected turn completion; events={events:#?}"
        );
        // The streamed reply must be consolidated into a persisted assistant
        // message so it survives a conversation reload (the reported bug: ACP
        // deltas alone left the reply gone when the chat was reopened).
        let persisted: Option<String> = {
            let conn = db.get_connection().expect("db connection");
            conn.query_row(
                "SELECT content FROM messages
                 WHERE runtime_session_id = ?1 AND role = 'assistant'
                 ORDER BY sequence DESC LIMIT 1",
                rusqlite::params![session.id],
                |row| row.get(0),
            )
            .optional()
            .expect("query persisted assistant message")
        };
        assert_eq!(
            persisted.as_deref(),
            Some("acp hello"),
            "assistant reply must be persisted to `messages`"
        );
        // Full access auto-approves without leaving the session awaiting approval.
        let reloaded = manager.get_session(&session.id).expect("reload session");
        assert_ne!(reloaded.status, AgentSessionStatus::AwaitingApproval);

        // The agent advertised a model config option; switching the model must
        // succeed and be remembered as this agent's preference for next time.
        manager
            .set_session_model(&session.id, "m-alt")
            .await
            .expect("set session model");
        assert_eq!(
            db.get_setting("acp_model_gemini_cli").expect("get setting"),
            Some("m-alt".to_string())
        );

        manager
            .stop_session(&session.id)
            .await
            .expect("stop ACP session");
        drop(manager);
        drop(db);
        let _ = std::fs::remove_file(script_path);
        let _ = std::fs::remove_file(worker_path);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn agent_process_crash_marks_session_failed() {
        let suffix = chrono::Utc::now().timestamp_micros();
        let db_path = std::env::temp_dir().join(format!("omnix_crash_manager_{suffix}.sqlite"));
        let script_path = std::env::temp_dir().join(format!("omnix_fake_crash_{suffix}.cmd"));
        // An agent that dies immediately after launch, without ever answering.
        std::fs::write(&script_path, "@echo off\r\nexit /b 3\r\n").expect("crash script");

        let db = Arc::new(DbManager::new_runtime_test(db_path.clone()));
        let conn = db.get_connection().expect("db connection");
        conn.execute(
            "INSERT INTO conversations (id, title, workspace_path, active_agent) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["conv-crash", "Crash", "D:/work/project", "Claude Code"],
        )
        .expect("conversation seed");
        drop(conn);

        let manager = RuntimeManager::new(Arc::clone(&db));
        let session = manager
            .start_session(AgentSessionConfig {
                conversation_id: "conv-crash".into(),
                agent: AgentId::ClaudeCode,
                executable_path: script_path.to_string_lossy().into_owned(),
                workspace_path: std::env::temp_dir().to_string_lossy().into_owned(),
                model: ModelSelection::AgentDefault,
                permission: PermissionPolicy::AskOnRisk,
                work_mode: WorkMode::Direct,
            })
            .await
            .expect("start crash session");

        let mut reloaded = manager.get_session(&session.id).expect("reload session");
        for _ in 0..60 {
            reloaded = manager.get_session(&session.id).expect("reload session");
            if reloaded.status == AgentSessionStatus::Failed {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(
            reloaded.status,
            AgentSessionStatus::Failed,
            "unexpected exit must mark the session failed"
        );
        let events = manager.list_events(&session.id).expect("list events");
        assert!(events.iter().any(|event| {
            event.kind == RuntimeEventKind::Error
                && event
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("意外退出"))
        }));

        drop(manager);
        drop(db);
        let _ = std::fs::remove_file(script_path);
        let _ = std::fs::remove_file(db_path);
    }
}
