use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex as AsyncMutex, RwLock};

use crate::db::DbManager;
use crate::runtime::{
    build_claude_user_message, build_codex_approval_response, build_codex_initialize_request,
    build_codex_thread_resume_request, build_codex_thread_start_request,
    build_codex_turn_start_request, build_launch_spec, build_resume_launch_spec,
    create_agent_session_record, get_agent_session_record, list_runtime_events, parse_claude_event,
    parse_codex_message, record_runtime_event, record_user_message, update_agent_session_status,
    AgentId, AgentSessionConfig, AgentSessionRecord, AgentSessionStatus, RuntimeEvent,
    RuntimeEventKind,
};

struct ActiveSession {
    config: AgentSessionConfig,
    stdin: AsyncMutex<ChildStdin>,
    child: Mutex<Child>,
    external_session_id: RwLock<Option<String>>,
    next_request_id: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEventEnvelope {
    pub session_id: String,
    pub event: RuntimeEvent,
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
            self.events.clone(),
        );
        spawn_log_reader(
            BufReader::new(stderr),
            session_id.to_string(),
            Arc::clone(&self.db),
            self.events.clone(),
        );

        if config.agent == AgentId::Codex {
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

            // Wait for the thread id before declaring the session ready. Codex
            // can take several seconds to answer `thread/start` while it boots
            // MCP servers, so surfacing readiness (or a clear failure) here keeps
            // the "starting" state honest and makes the first message instant.
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

        Ok(())
    }

    #[cfg(test)]
    pub async fn send_message(&self, session_id: &str, prompt: &str) -> Result<(), String> {
        self.send_message_with_display(session_id, prompt, prompt)
            .await
    }

    pub async fn send_message_with_display(
        &self,
        session_id: &str,
        prompt: &str,
        display_text: &str,
    ) -> Result<(), String> {
        let active = self.active_session(session_id).await?;
        record_user_message(&self.db, session_id, display_text)?;
        match active.config.agent {
            AgentId::ClaudeCode => {
                write_json_line(&active, &build_claude_user_message(prompt)).await
            }
            AgentId::Codex => {
                let thread_id = wait_for_external_session(&active).await?;
                let request_id = active.next_request_id.fetch_add(1, Ordering::Relaxed);
                let request =
                    build_codex_turn_start_request(request_id, &thread_id, prompt, &active.config)?;
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
        if active.config.agent != AgentId::Codex {
            return Err("Claude Code 的结构化审批回传尚未验证；请改用计划模式或 Codex".into());
        }
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

    pub async fn stop_session(&self, session_id: &str) -> Result<(), String> {
        update_agent_session_status(&self.db, session_id, AgentSessionStatus::Stopping, None)?;
        let active = self.active.write().await.remove(session_id);
        if let Some(active) = active {
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
            return command;
        }
    }
    let mut command = Command::new(program);
    command.args(args);
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
        // Detect a dead Codex process eagerly so we surface an actionable error
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
                "Codex app-server 在返回线程 ID 前已退出（{status}）。请检查 ~/.codex/config.toml 中的 MCP 服务配置后重试。"
            ));
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Codex app-server 在 {} 秒内未返回线程 ID。常见原因是 ~/.codex/config.toml 配置了启动缓慢或失效的 MCP 服务；请检查后重试。",
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
    events: broadcast::Sender<SessionEventEnvelope>,
) where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let parsed = match agent {
                AgentId::ClaudeCode => parse_claude_event(&line),
                AgentId::Codex => parse_codex_message(&line),
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
    });
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
}
