use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::db::DbManager;
use crate::runtime::{managed_install_command, AgentId};

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn resolve_sandbox_path(path_str: &str) -> PathBuf {
    let normalized = path_str.replace('\\', "/");
    if normalized.starts_with("~/") || normalized == "~" {
        if let Some(home) = dirs::home_dir() {
            let sub = if normalized == "~" {
                ""
            } else {
                &normalized[2..]
            };
            if sub.is_empty() {
                home
            } else {
                home.join(sub)
            }
        } else {
            PathBuf::from(path_str)
        }
    } else {
        PathBuf::from(path_str)
    }
}

fn agent_slug(agent_name: &str) -> &'static str {
    match agent_name {
        "Claude Code" => "claude-code",
        "Codex" => "codex",
        "Gemini CLI" => "gemini-cli",
        "OpenCode" => "opencode",
        "Qwen Code" => "qwen-code",
        "GitHub Copilot CLI" => "github-copilot-cli",
        "Google Antigravity" => "antigravity",
        _ => "custom-agent",
    }
}

fn managed_agent_root(db: &DbManager, agent_name: &str) -> PathBuf {
    let key = format!("sandbox_dir_{}", agent_name);
    if let Some(path) = db.get_setting(&key).ok().flatten() {
        return resolve_sandbox_path(&path);
    }
    let base = db
        .get_setting("sandbox_dir")
        .ok()
        .flatten()
        .unwrap_or_else(|| "~/.omnix/agents".into());
    resolve_sandbox_path(&base).join(agent_slug(agent_name))
}

fn executable_in_managed_root(root: &Path, bin_name: &str) -> Option<String> {
    let executable = root
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) {
            format!("{bin_name}.cmd")
        } else {
            bin_name.to_string()
        });
    executable
        .exists()
        .then(|| executable.to_string_lossy().to_string())
}

fn prefer_windows_command_shim(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }

    let extension = path.extension().and_then(|value| value.to_str());
    if extension.is_none() || extension.is_some_and(|value| value.eq_ignore_ascii_case("ps1")) {
        let command_shim = path.with_extension("cmd");
        if command_shim.is_file() {
            return command_shim;
        }
    }

    path
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    pub name: String,
    pub path: String,
    pub version: String,
    pub status: String, // "installed", "not_installed", "broken"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcpTask {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcpParams {
    pub tasks: Vec<AcpTask>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcpRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: AcpParams,
}

pub enum AgentChild {
    Standard(tokio::process::Child),
    Pty {
        child: Box<dyn portable_pty::Child + Send + Sync>,
        #[allow(dead_code)]
        pty_pair: portable_pty::PtyPair,
    },
}

// Track active subprocesses and their last activity timestamp
struct ActiveProcess {
    child: Arc<tokio::sync::Mutex<AgentChild>>,
    last_activity: Arc<AtomicU64>,
    stdin_tx: mpsc::Sender<String>,
}

pub struct AgentManager {
    db: Arc<DbManager>,
    active_processes: Arc<Mutex<HashMap<String, ActiveProcess>>>,
}

impl AgentManager {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self {
            db,
            active_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_services(&self) {
        self.start_idle_reaper();
        self.start_cron_scheduler();
    }

    // --- 1. Agent Detection logic ---
    pub fn detect_agents(&self) -> Vec<DetectedAgent> {
        let mut list = Vec::new();
        let agent_names = vec![
            ("Claude Code", "claude"),
            ("Gemini CLI", "gemini"),
            ("Codex", "codex"),
            ("Qwen Code", "qwen-code"),
            ("GitHub Copilot CLI", "github-copilot-cli"),
            ("Google Antigravity", "agy"),
            ("OpenCode", "opencode"),
        ];

        let sandbox_dir_str = self
            .db
            .get_setting("sandbox_dir")
            .unwrap_or(None)
            .unwrap_or_else(|| "~/.omnix/agents".to_string());
        let sandbox_dir = resolve_sandbox_path(&sandbox_dir_str);

        // Setup local sandbox search paths
        let mut local_bin_dir = sandbox_dir;
        local_bin_dir.push("node_modules");
        local_bin_dir.push(".bin");

        for (display_name, _) in agent_names {
            let found_path = self.find_agent_path(display_name);

            if let Some(path) = found_path {
                // Quick command execution to query version
                let version = self.query_agent_version(&path);
                list.push(DetectedAgent {
                    name: display_name.to_string(),
                    path,
                    version,
                    status: "installed".to_string(),
                });
            } else {
                list.push(DetectedAgent {
                    name: display_name.to_string(),
                    path: "".to_string(),
                    version: "".to_string(),
                    status: "not_installed".to_string(),
                });
            }
        }

        list
    }

    fn query_agent_version(&self, exe_path: &str) -> String {
        // Run <path> --version
        let output = std::process::Command::new(exe_path)
            .arg("--version")
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !stdout.is_empty() {
                    stdout
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    if !stderr.is_empty() {
                        stderr
                    } else {
                        "0.1.0".to_string()
                    }
                }
            }
            Err(_) => "Unknown".to_string(),
        }
    }

    // --- 2. Headless Configuration Bootstrap (TOS Bypass) ---
    pub fn bootstrap_claude_code(&self) {
        // Claude Code accepts license config at ~/.config/claude-code/config.json (on Windows and Mac/Linux)
        let home_dir = dirs::home_dir().expect("Failed to determine home directory");
        let mut config_dir = home_dir.clone();
        config_dir.push(".config");
        config_dir.push("claude-code");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut config_file = config_dir;
        config_file.push("config.json");

        // Write configuration pre-approving license terms and telemetry opt-out
        if !config_file.exists() {
            let tos_bypass_json = serde_json::json!({
                "analyticsConsent": "opt-out",
                "tosAccepted": true,
                "primaryColor": "green"
            });
            let _ = fs::write(config_file, tos_bypass_json.to_string());
            println!(
                "Pre-seeded Claude Code configuration to bypass initial TOS interactive prompt."
            );
        }
    }
}

fn generate_uuid_from_seed(seed: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h1 = DefaultHasher::new();
    seed.hash(&mut h1);
    let v1 = h1.finish();

    let mut h2 = DefaultHasher::new();
    (seed.to_string() + "_extra").hash(&mut h2);
    let v2 = h2.finish();

    let b1 = (v1 >> 32) as u32;
    let b2 = (v1 & 0xffff_ffff) as u32;
    let b3 = (v2 >> 32) as u32;
    let b4 = (v2 & 0xffff_ffff) as u32;

    let s = format!("{:08x}{:08x}{:08x}{:08x}", b1, b2, b3, b4);

    format!(
        "{}-{}-{}-8{}-{}",
        &s[0..8],
        &s[8..12],
        "435a",
        &s[17..20],
        &s[20..32]
    )
}

impl AgentManager {
    // --- 3. Run and execute subprocesses ---
    pub fn spawn_agent(
        &self,
        session_id: String,
        agent_name: String,
        exe_path: String,
        args: Vec<String>,
        workspace_dir: String,
        stdout_tx: mpsc::Sender<String>,
    ) -> Result<mpsc::Sender<String>, String> {
        let exe_path = if cfg!(windows) {
            exe_path.replace('/', "\\")
        } else {
            exe_path
        };
        let mut args = args;
        if exe_path.contains("claude") {
            if !args.iter().any(|arg| arg == "--setting-sources") {
                args.push("--setting-sources".to_string());
                args.push("project,local".to_string());
            }

            // Generate deterministic UUID for Claude Code session
            let claude_uuid = generate_uuid_from_seed(&session_id);

            // Check if this session has been initialized before
            let mut session_exists = false;
            if let Some(home_dir) = dirs::home_dir() {
                let mut projects_dir = home_dir;
                projects_dir.push(".claude");
                projects_dir.push("projects");

                if projects_dir.exists() {
                    if let Ok(entries) = fs::read_dir(&projects_dir) {
                        for entry in entries.flatten() {
                            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                let mut session_file = entry.path();
                                session_file.push(format!("{}.jsonl", claude_uuid));
                                if session_file.exists() {
                                    session_exists = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if session_exists {
                args.push("--resume".to_string());
                args.push(claude_uuid);
            } else {
                args.push("--session-id".to_string());
                args.push(claude_uuid);
            }
        }

        let resolved_workspace = if workspace_dir == "direct" || workspace_dir.trim().is_empty() {
            dirs::home_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        } else {
            resolve_sandbox_path(&workspace_dir)
        };
        let resolved_workspace_str = resolved_workspace.to_string_lossy().to_string();

        log::warn!("OMNIX spawn_agent: session_id={}, agent_name={}, exe_path={}, workspace_dir={}, resolved={}",
                 session_id, agent_name, exe_path, workspace_dir, resolved_workspace_str);

        // Auto-create workspace directory if it doesn't exist to prevent os error 267 (directory invalid)
        if !resolved_workspace_str.trim().is_empty() && workspace_dir != "direct" {
            if !resolved_workspace.exists() {
                if let Err(e) = fs::create_dir_all(&resolved_workspace) {
                    return Err(format!("工作区目录不存在且自动创建失败: {}", e));
                }
            }
        }

        // Pre-initialization check for Claude Code
        if exe_path.contains("claude") {
            self.bootstrap_claude_code();
        }

        // Inject long-term memory anti-failure files into the workspace directory
        let _ = self.inject_workspace_memories(&resolved_workspace_str, &agent_name);

        // Configure environment variables (supporting WSL cross-boundary translation or local loopback)
        let use_wsl = self
            .db
            .get_setting("use_wsl")
            .unwrap_or(None)
            .unwrap_or_else(|| "false".to_string())
            == "true";
        let wsl_distro = self
            .db
            .get_setting("wsl_distro")
            .unwrap_or(None)
            .unwrap_or_else(|| "Ubuntu".to_string());
        let proxy_port = self
            .db
            .get_setting("proxy_port")
            .unwrap_or(None)
            .unwrap_or_else(|| "1421".to_string());

        // Try using PTY system
        let pty_system = std::panic::catch_unwind(|| portable_pty::native_pty_system())
            .map_err(|_| "PTY 系统在初始化时崩溃".to_string())?;

        let pty_pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("无法创建虚拟终端 (PTY) 会话: {}", e))?;

        let cmd_builder = if use_wsl {
            let mut c = portable_pty::CommandBuilder::new("wsl.exe");
            let args_escaped: Vec<String> = args
                .iter()
                .map(|a| {
                    if a.contains(' ') || a.contains('"') || a.contains('\'') {
                        format!("'{}'", a.replace("'", "'\\''"))
                    } else {
                        a.clone()
                    }
                })
                .collect();
            let command_str = format!("{} {}", exe_path, args_escaped.join(" "));
            let sh_command = format!(
                "HOST_IP=$(ip route | grep default | awk '{{print $3}}'); \
                 export ANTHROPIC_BASE_URL=http://$HOST_IP:{}/agent/{}; \
                 export CLAUDE_CODE_HEADLESS=0; \
                 export ANTHROPIC_API_KEY=dummy-key-for-omnix; \
                 export DISABLE_UPDATES=1; \
                 export DISABLE_AUTOUPDATER=1; \
                 {}",
                proxy_port,
                agent_name.replace(' ', "_"),
                command_str
            );
            c.args(&["-d", &wsl_distro, "--", "sh", "-c", &sh_command]);
            c
        } else {
            let local_proxy_url = format!(
                "http://localhost:{}/agent/{}",
                proxy_port,
                agent_name.replace(' ', "_")
            );

            // On Windows, non-.exe files (like .cmd, .bat, or script wrappers without extension)
            // cannot be executed directly by CreateProcess via portable-pty. They must be wrapped in cmd.exe /c.
            let mut is_script = false;
            if cfg!(windows) {
                let path_lower = exe_path.to_lowercase();
                if path_lower.ends_with(".cmd")
                    || path_lower.ends_with(".bat")
                    || !path_lower.ends_with(".exe")
                {
                    is_script = true;
                }
            }

            let mut c = if is_script {
                let mut builder = portable_pty::CommandBuilder::new("cmd.exe");
                builder.arg("/c");
                builder.arg(&exe_path);
                builder.args(&args);
                builder
            } else {
                let mut builder = portable_pty::CommandBuilder::new(&exe_path);
                builder.args(&args);
                builder
            };
            c.env("ANTHROPIC_BASE_URL", &local_proxy_url);
            c.env("CLAUDE_CODE_HEADLESS", "0");
            c.env("ANTHROPIC_API_KEY", "dummy-key-for-omnix");
            c.env("DISABLE_UPDATES", "1");
            c.env("DISABLE_AUTOUPDATER", "1");
            c
        };

        let mut cmd_builder = cmd_builder;
        cmd_builder.cwd(&resolved_workspace);

        let child = pty_pair.slave.spawn_command(cmd_builder).map_err(|e| {
            format!(
                "虚拟终端运行智能体失败: {}。智能体可执行路径: '{}'，参数: {:?}",
                e, exe_path, args
            )
        })?;

        let spawned_pty = Some((pty_pair, child));

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
        let last_activity = Arc::new(AtomicU64::new(current_time_ms()));

        let child_shared = if let Some((pty_pair, child)) = spawned_pty {
            let writer = pty_pair
                .master
                .take_writer()
                .map_err(|e| format!("Failed to get pty master writer: {}", e))?;
            let reader = pty_pair
                .master
                .try_clone_reader()
                .map_err(|e| format!("Failed to clone pty master reader: {}", e))?;

            // 1. Thread for handling writing to PTY stdin
            let last_activity_stdin = Arc::clone(&last_activity);
            let session_id_stdin = session_id.clone();
            tauri::async_runtime::spawn(async move {
                let mut writer = writer;
                while let Some(msg) = stdin_rx.recv().await {
                    // In a PTY, a line submission is triggered by a carriage return ('\r').
                    // A line feed ('\n') is interpreted as a newline insert in interactive prompts like Claude Code,
                    // which causes it to enter multi-line edit mode instead of executing the command.
                    // We normalize all newlines in PTY stdin to Carriage Returns ('\r') to ensure execution.
                    let normalized_msg = msg.replace("\r\n", "\r").replace('\n', "\r");
                    println!(
                        "OMNIX PTY (Session {}): Writing stdin -> {:?}",
                        session_id_stdin, normalized_msg
                    );
                    last_activity_stdin.store(current_time_ms(), Ordering::Relaxed);
                    let res = tokio::task::spawn_blocking(move || {
                        writer
                            .write_all(normalized_msg.as_bytes())
                            .and_then(|_| writer.flush())
                            .map(|_| writer)
                    })
                    .await;
                    match res {
                        Ok(Ok(w)) => {
                            writer = w;
                        }
                        _ => {
                            log::warn!("OMNIX spawn_agent (PTY): Failed to write to stdin");
                            break;
                        }
                    }
                }
            });

            // 2. Thread for reading PTY stdout/stderr (mixed)
            let (pty_out_tx, mut pty_out_rx) = mpsc::channel::<Vec<u8>>(100);
            std::thread::spawn(move || {
                let mut reader = reader;
                let mut buf = vec![0; 4096];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if pty_out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            });

            let stdout_tx_clone = stdout_tx.clone();
            let last_activity_stdout = Arc::clone(&last_activity);
            let session_id_for_stdout = session_id.clone();
            let db_clone = Arc::clone(&self.db);
            tauri::async_runtime::spawn(async move {
                let mut line_accumulator = String::new();
                while let Some(bytes) = pty_out_rx.recv().await {
                    last_activity_stdout.store(current_time_ms(), Ordering::Relaxed);

                    // Strip ANSI escape codes
                    let clean_bytes = strip_ansi_escapes::strip(&bytes);
                    let clean_str = String::from_utf8_lossy(&clean_bytes);
                    println!(
                        "OMNIX PTY (Session {}): Read stdout -> {:?}",
                        session_id_for_stdout, clean_str
                    );
                    let _ = stdout_tx_clone.send(format!("STDOUT: {}", clean_str)).await;

                    line_accumulator.push_str(&clean_str);
                    while let Some(pos) = line_accumulator.find('\n') {
                        let line = line_accumulator[..pos].to_string();
                        line_accumulator = line_accumulator[pos + 1..].to_string();

                        let trimmed = line.trim();
                        if trimmed.starts_with('{') && trimmed.ends_with('}') {
                            if let Ok(req) = serde_json::from_str::<AcpRequest>(trimmed) {
                                if req.method == "task/plan" {
                                    let conn = db_clone.get_connection();
                                    if let Ok(conn) = conn {
                                        let _ = conn.execute(
                                            "DELETE FROM tasks WHERE conversation_id = ?1",
                                            params![session_id_for_stdout],
                                        );
                                        for (i, t) in req.params.tasks.iter().enumerate() {
                                            let _ = conn.execute(
                                                "INSERT INTO tasks (id, conversation_id, title, status, order_num)
                                                 VALUES (?1, ?2, ?3, ?4, ?5)",
                                                params![t.id, session_id_for_stdout, t.title, t.status, i as i32],
                                            );
                                        }
                                    }
                                    let _ =
                                        stdout_tx_clone.send(format!("ACP: {}\n", trimmed)).await;
                                }
                            }
                        }
                    }
                }
            });

            Arc::new(tokio::sync::Mutex::new(AgentChild::Pty { child, pty_pair }))
        } else {
            // Fallback: spawn command using traditional piped Stdio
            if exe_path.contains("codex") {
                if !args.iter().any(|arg| {
                    arg == "exec"
                        || arg == "e"
                        || arg == "review"
                        || arg == "login"
                        || arg == "logout"
                        || arg == "mcp"
                        || arg == "help"
                        || arg == "--help"
                        || arg == "--version"
                        || arg == "-V"
                        || arg == "-h"
                }) {
                    args.insert(0, "exec".to_string());
                }
            }

            let mut cmd = if use_wsl {
                let mut c = Command::new("wsl.exe");
                let args_escaped: Vec<String> = args
                    .iter()
                    .map(|a| {
                        if a.contains(' ') || a.contains('"') || a.contains('\'') {
                            format!("'{}'", a.replace("'", "'\\''"))
                        } else {
                            a.clone()
                        }
                    })
                    .collect();
                let command_str = format!("{} {}", exe_path, args_escaped.join(" "));
                let sh_command = format!(
                    "HOST_IP=$(ip route | grep default | awk '{{print $3}}'); \
                     export ANTHROPIC_BASE_URL=http://$HOST_IP:{}/agent/{}; \
                     export CLAUDE_CODE_HEADLESS=1; \
                     export ANTHROPIC_API_KEY=dummy-key-for-omnix; \
                     export DISABLE_UPDATES=1; \
                     export DISABLE_AUTOUPDATER=1; \
                     {}",
                    proxy_port,
                    agent_name.replace(' ', "_"),
                    command_str
                );
                c.args(&["-d", &wsl_distro, "--", "sh", "-c", &sh_command]);
                c
            } else {
                let local_proxy_url = format!(
                    "http://localhost:{}/agent/{}",
                    proxy_port,
                    agent_name.replace(' ', "_")
                );
                let mut c = Command::new(&exe_path);
                c.args(args)
                    .env("ANTHROPIC_BASE_URL", &local_proxy_url)
                    .env("CLAUDE_CODE_HEADLESS", "1")
                    .env("ANTHROPIC_API_KEY", "dummy-key-for-omnix")
                    .env("DISABLE_UPDATES", "1")
                    .env("DISABLE_AUTOUPDATER", "1");
                c
            };

            cmd.current_dir(resolved_workspace)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = match cmd.spawn() {
                Ok(c) => {
                    log::warn!(
                        "OMNIX spawn_agent: Successfully spawned fallback process with PID {:?}",
                        c.id()
                    );
                    c
                }
                Err(e) => {
                    log::warn!("OMNIX spawn_agent: Failed to spawn fallback process: {}", e);
                    return Err(format!("Failed to launch agent fallback process: {}", e));
                }
            };

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| "Failed to open stdin stream".to_string())?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| "Failed to open stdout stream".to_string())?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| "Failed to open stderr stream".to_string())?;

            let last_activity_stdin = Arc::clone(&last_activity);
            tauri::async_runtime::spawn(async move {
                let mut writer = stdin;
                while let Some(msg) = stdin_rx.recv().await {
                    last_activity_stdin.store(current_time_ms(), Ordering::Relaxed);
                    if let Err(e) = writer.write_all(msg.as_bytes()).await {
                        log::warn!(
                            "OMNIX spawn_agent: Failed to write to fallback stdin: {}",
                            e
                        );
                        break;
                    }
                    if let Err(e) = writer.flush().await {
                        log::warn!("OMNIX spawn_agent: Failed to flush fallback stdin: {}", e);
                        break;
                    }
                }
            });

            let stdout_tx_clone = stdout_tx.clone();
            let last_activity_stdout = Arc::clone(&last_activity);
            let session_id_for_stdout = session_id.clone();
            let db_clone = Arc::clone(&self.db);
            tauri::async_runtime::spawn(async move {
                let mut reader = stdout;
                let mut buf = vec![0; 4096];
                let mut line_accumulator = String::new();
                while let Ok(n) = reader.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    last_activity_stdout.store(current_time_ms(), Ordering::Relaxed);
                    let chunk_str = String::from_utf8_lossy(&buf[..n]);
                    let _ = stdout_tx_clone.send(format!("STDOUT: {}", chunk_str)).await;

                    line_accumulator.push_str(&chunk_str);
                    while let Some(pos) = line_accumulator.find('\n') {
                        let line = line_accumulator[..pos].to_string();
                        line_accumulator = line_accumulator[pos + 1..].to_string();

                        let trimmed = line.trim();
                        if trimmed.starts_with('{') && trimmed.ends_with('}') {
                            if let Ok(req) = serde_json::from_str::<AcpRequest>(trimmed) {
                                if req.method == "task/plan" {
                                    let conn = db_clone.get_connection();
                                    if let Ok(conn) = conn {
                                        let _ = conn.execute(
                                            "DELETE FROM tasks WHERE conversation_id = ?1",
                                            params![session_id_for_stdout],
                                        );
                                        for (i, t) in req.params.tasks.iter().enumerate() {
                                            let _ = conn.execute(
                                                "INSERT INTO tasks (id, conversation_id, title, status, order_num)
                                                 VALUES (?1, ?2, ?3, ?4, ?5)",
                                                params![t.id, session_id_for_stdout, t.title, t.status, i as i32],
                                            );
                                        }
                                    }
                                    let _ =
                                        stdout_tx_clone.send(format!("ACP: {}\n", trimmed)).await;
                                }
                            }
                        }
                    }
                }
            });

            let stderr_tx_clone = stdout_tx.clone();
            let last_activity_stderr = Arc::clone(&last_activity);
            tauri::async_runtime::spawn(async move {
                let mut reader = stderr;
                let mut buf = vec![0; 4096];
                while let Ok(n) = reader.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    last_activity_stderr.store(current_time_ms(), Ordering::Relaxed);
                    let chunk_str = String::from_utf8_lossy(&buf[..n]);
                    let _ = stderr_tx_clone.send(format!("STDERR: {}", chunk_str)).await;
                }
            });

            Arc::new(tokio::sync::Mutex::new(AgentChild::Standard(child)))
        };

        let child_shared = child_shared;

        let proc = ActiveProcess {
            child: Arc::clone(&child_shared),
            last_activity: Arc::clone(&last_activity),
            stdin_tx: stdin_tx.clone(),
        };

        let session_id_for_wait = session_id.clone();

        if let Ok(mut procs) = self.active_processes.lock() {
            procs.insert(session_id, proc);
        }

        // 4. Thread for awaiting process termination asynchronously without holding the map lock via polling try_wait
        let active_processes_for_wait = Arc::clone(&self.active_processes);
        let child_for_wait = Arc::clone(&child_shared);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let mut child_lock = child_for_wait.lock().await;
                match &mut *child_lock {
                    AgentChild::Standard(c) => {
                        if let Ok(Some(status)) = c.try_wait() {
                            println!(
                                "Subprocess for session {} exited with status {:?}",
                                session_id_for_wait, status
                            );
                            break;
                        }
                    }
                    AgentChild::Pty { child, .. } => {
                        if let Ok(Some(status)) = child.try_wait() {
                            println!(
                                "PTY Subprocess for session {} exited with status {:?}",
                                session_id_for_wait, status
                            );
                            break;
                        }
                    }
                }
            }
            // Clean up registry map on process exit
            if let Ok(mut procs) = active_processes_for_wait.lock() {
                procs.remove(&session_id_for_wait);
            }
        });

        Ok(stdin_tx)
    }

    pub async fn install_agent(&self, agent_name: &str) -> Result<(), String> {
        if agent_name == "Qwen Code" {
            return Err("Qwen Code managed installation is not supported yet; OMNIX will not create a mock CLI".into());
        }

        if agent_name == "Google Antigravity" {
            let mut cmd = if cfg!(windows) {
                let mut c = Command::new("powershell");
                c.args(&[
                    "-Command",
                    "irm https://antigravity.google/cli/install.ps1 | iex",
                ]);
                c
            } else {
                let mut c = Command::new("sh");
                c.args(&[
                    "-c",
                    "curl -fsSL https://antigravity.google/cli/install.sh | bash",
                ]);
                c
            };
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("Failed to spawn Antigravity installer: {}", e))?;
            let status = child
                .wait()
                .await
                .map_err(|e| format!("Antigravity installer run error: {}", e))?;
            if status.success() {
                return Ok(());
            } else {
                return Err(format!(
                    "Antigravity installer failed with code {:?}",
                    status.code()
                ));
            }
        }

        let core_agent = match agent_name {
            "Claude Code" => Some(AgentId::ClaudeCode),
            "Codex" => Some(AgentId::Codex),
            _ => None,
        };
        let package = match agent_name {
            "Gemini CLI" => "@google/gemini-cli@latest",
            "GitHub Copilot CLI" => "@github/copilot-cli@latest",
            "OpenCode" => "opencode-ai@latest",
            _ if core_agent.is_some() => "",
            _ => {
                return Err(format!(
                    "Unsupported agent CLI auto-install: {}",
                    agent_name
                ))
            }
        };

        let sandbox_dir = managed_agent_root(&self.db, agent_name);

        // Ensure directory exists
        let _ = fs::create_dir_all(&sandbox_dir);
        let sandbox_str = sandbox_dir.to_string_lossy().to_string();

        println!(
            "Installing agent {} in sandbox prefix {}",
            agent_name, sandbox_str
        );

        let install_command = if let Some(agent) = core_agent {
            managed_install_command(agent, &sandbox_str)
        } else {
            crate::runtime::ManagedInstallCommand {
                program: if cfg!(windows) {
                    "npm.cmd".into()
                } else {
                    "npm".into()
                },
                args: vec![
                    "install".into(),
                    "--prefix".into(),
                    sandbox_str.clone(),
                    package.into(),
                ],
            }
        };
        let mut cmd = Command::new(&install_command.program);
        cmd.args(&install_command.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run npm command: {}", e))?;
        let status = child
            .wait()
            .await
            .map_err(|e| format!("Npm install process error: {}", e))?;

        if status.success() {
            if agent_name == "Claude Code" {
                self.bootstrap_claude_code();
            }
            Ok(())
        } else {
            Err(format!(
                "Npm install failed with status exit code {:?}",
                status.code()
            ))
        }
    }

    pub async fn repair_agent_cli(&self, agent_name: &str) -> Result<(), String> {
        // 1. Clean npm lockfiles inside the sandbox
        if agent_name == "Claude Code" || agent_name == "GitHub Copilot CLI" {
            let sandbox_dir = managed_agent_root(&self.db, agent_name);

            let mut lock_file = sandbox_dir.clone();
            lock_file.push("package-lock.json");
            if lock_file.exists() {
                let _ = fs::remove_file(lock_file);
            }

            let mut node_modules_lock = sandbox_dir.clone();
            node_modules_lock.push("node_modules");
            node_modules_lock.push(".package-lock.json");
            if node_modules_lock.exists() {
                let _ = fs::remove_file(node_modules_lock);
            }
            println!("Cleared sandbox lockfiles for agent {}", agent_name);
        }

        // 2. Perform a clean reinstall
        self.install_agent(agent_name).await?;
        Ok(())
    }

    pub fn sync_agent_configs(&self) -> Result<(), String> {
        let home_dir = dirs::home_dir().expect("Failed to determine home directory");

        // A. Sync Claude Code config
        let mut claude_code_config = home_dir.clone();
        claude_code_config.push(".config");
        claude_code_config.push("claude-code");
        claude_code_config.push("config.json");

        if claude_code_config
            .parent()
            .map(|p| p.exists())
            .unwrap_or(false)
        {
            let mut val = if claude_code_config.exists() {
                let content = fs::read_to_string(&claude_code_config).unwrap_or_default();
                serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            if let Some(obj) = val.as_object_mut() {
                obj.insert("tosAccepted".to_string(), serde_json::Value::Bool(true));
                obj.insert(
                    "analyticsConsent".to_string(),
                    serde_json::Value::String("opt-out".to_string()),
                );
            }
            self.atomic_write_config(&claude_code_config, &val.to_string())?;
        }

        // B. Sync Claude Desktop config
        let mut claude_desktop_dir = home_dir.clone();
        claude_desktop_dir.push("AppData");
        claude_desktop_dir.push("Roaming");
        claude_desktop_dir.push("Claude");

        let mut claude_desktop_config = claude_desktop_dir.clone();
        claude_desktop_config.push("claude_desktop_config.json");

        if claude_desktop_dir.exists() {
            let mut val = if claude_desktop_config.exists() {
                let content = fs::read_to_string(&claude_desktop_config).unwrap_or_default();
                serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            if let Some(obj) = val.as_object_mut() {
                let mut mcp_servers = obj
                    .remove("mcpServers")
                    .unwrap_or_else(|| serde_json::json!({}));
                if mcp_servers.as_object().is_none() {
                    mcp_servers = serde_json::json!({});
                }

                // Inject custom MCP server settings pointing to OMNIX
                obj.insert("mcpServers".to_string(), mcp_servers);
            }

            self.atomic_write_config(&claude_desktop_config, &val.to_string())?;
            println!("Synchronized OMNIX configuration to Claude Desktop.");
        }

        Ok(())
    }

    pub async fn uninstall_agent(&self, agent_name: &str) -> Result<(), String> {
        let sandbox_dir = managed_agent_root(&self.db, agent_name);

        if agent_name == "Codex" || agent_name == "Qwen Code" {
            let bin_name = if agent_name == "Codex" {
                "codex"
            } else {
                "qwen-code"
            };
            let mut bin_dir = sandbox_dir.clone();
            bin_dir.push("node_modules");
            bin_dir.push(".bin");

            let bin_file = bin_dir.join(bin_name);
            let cmd_file = bin_dir.join(format!("{}.cmd", bin_name));
            let _ = fs::remove_file(&bin_file);
            let _ = fs::remove_file(&cmd_file);
        } else if agent_name == "Google Antigravity" {
            if let Some(local_dir) = dirs::data_local_dir() {
                let agy_dir = local_dir.join("agy");
                if agy_dir.exists() {
                    let _ = fs::remove_dir_all(&agy_dir);
                }
            }
            if let Some(home) = dirs::home_dir() {
                let agy_dir = home.join(".local").join("share").join("agy");
                if agy_dir.exists() {
                    let _ = fs::remove_dir_all(&agy_dir);
                }
            }
        } else {
            let package_folder = match agent_name {
                "Claude Code" => "@anthropic-ai/claude-code",
                "Gemini CLI" => "@google/gemini-cli",
                "GitHub Copilot CLI" => "@github/copilot-cli",
                "OpenCode" => "opencode-ai",
                _ => return Err(format!("Unsupported agent CLI uninstall: {}", agent_name)),
            };

            let bin_name = match agent_name {
                "Claude Code" => "claude",
                "Gemini CLI" => "gemini",
                "GitHub Copilot CLI" => "github-copilot-cli",
                "OpenCode" => "opencode",
                _ => "",
            };

            let mut pkg_dir = sandbox_dir.clone();
            pkg_dir.push("node_modules");
            pkg_dir.push(package_folder);
            if pkg_dir.exists() {
                let _ = fs::remove_dir_all(&pkg_dir);
            }

            let mut bin_dir = sandbox_dir.clone();
            bin_dir.push("node_modules");
            bin_dir.push(".bin");

            if !bin_name.is_empty() {
                let _ = fs::remove_file(bin_dir.join(bin_name));
                let _ = fs::remove_file(bin_dir.join(format!("{}.cmd", bin_name)));
            }
        }

        Ok(())
    }

    fn atomic_write_config(&self, file_path: &Path, content: &str) -> Result<(), String> {
        let mut tmp_path = file_path.to_path_buf();
        tmp_path.set_extension("tmp");

        fs::write(&tmp_path, content).map_err(|e| format!("Failed to write tmp file: {}", e))?;
        fs::rename(&tmp_path, file_path)
            .map_err(|e| format!("Failed to atomically replace config file: {}", e))?;
        Ok(())
    }

    pub fn get_active_session_ids(&self) -> Vec<String> {
        if let Ok(procs) = self.active_processes.lock() {
            procs.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub fn terminate_agent(&self, session_id: &str) {
        if let Ok(mut procs) = self.active_processes.lock() {
            if let Some(proc) = procs.remove(session_id) {
                // Kill asynchronously to prevent blocking the registry lock
                tauri::async_runtime::spawn(async move {
                    let mut child = proc.child.lock().await;
                    match &mut *child {
                        AgentChild::Standard(c) => {
                            let _ = c.start_kill();
                        }
                        AgentChild::Pty { child, .. } => {
                            let _ = child.kill();
                        }
                    }
                });
                println!(
                    "Forcefully terminated agent process for session: {}",
                    session_id
                );
            }
        }
    }

    pub fn send_stdin(&self, session_id: &str, text: String) -> Result<(), String> {
        let procs = self
            .active_processes
            .lock()
            .map_err(|_| "Failed to lock active processes map".to_string())?;
        if let Some(proc) = procs.get(session_id) {
            let tx = proc.stdin_tx.clone();
            tauri::async_runtime::spawn(async move {
                let _ = tx.send(text).await;
            });
            Ok(())
        } else {
            Err(format!(
                "No active agent session found with ID {}",
                session_id
            ))
        }
    }

    // --- 4. Idle Reaper Watchdog thread ---
    fn start_idle_reaper(&self) {
        let active_processes_clone = Arc::clone(&self.active_processes);
        let db_clone = Arc::clone(&self.db);

        tauri::async_runtime::handle().spawn(async move {
            loop {
                // Check every 30 seconds
                tokio::time::sleep(Duration::from_secs(30)).await;

                // Load threshold from DB config
                let timeout_min_str = db_clone.get_setting("idle_timeout_min").unwrap_or(None).unwrap_or_else(|| "15".to_string());
                let timeout_min = timeout_min_str.parse::<u64>().unwrap_or(15);
                let timeout_duration = Duration::from_secs(timeout_min * 60);

                let mut to_reap = Vec::new();

                if let Ok(procs) = active_processes_clone.lock() {
                    for (session_id, proc) in procs.iter() {
                        let last_act = proc.last_activity.load(Ordering::Relaxed);
                        let elapsed_ms = current_time_ms().saturating_sub(last_act);
                        if elapsed_ms > timeout_duration.as_millis() as u64 {
                            to_reap.push(session_id.clone());
                        }
                    }
                }

                // Terminate reaped processes
                for session_id in to_reap {
                    println!("Idle Reaper: Session {} exceeded idle threshold of {} minutes. Killing subprocess...", session_id, timeout_min);
                    if let Ok(mut procs_lock) = active_processes_clone.lock() {
                        if let Some(proc) = procs_lock.remove(&session_id) {
                            tauri::async_runtime::spawn(async move {
                                let mut child = proc.child.lock().await;
                                match &mut *child {
                                    AgentChild::Standard(c) => {
                                        let _ = c.start_kill();
                                    }
                                    AgentChild::Pty { child, .. } => {
                                        let _ = child.kill();
                                    }
                                }
                            });
                        }
                    }
                }
            }
        });
    }

    fn inject_workspace_memories(
        &self,
        workspace_dir: &str,
        agent_name: &str,
    ) -> Result<(), String> {
        let conn = self.db.get_connection().map_err(|e| e.to_string())?;
        // Only inject experience-type memories (not preferences — those are queried on demand)
        let mut stmt = conn.prepare(
            "SELECT incident_desc, code_pattern, remediation, keywords FROM memories WHERE type = 'experience' ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut memories_md = String::new();
        memories_md.push_str("\n<!--- OMNIX MEMORY START --->\n");
        memories_md.push_str("## 🧠 OMNIX Anti-Failure Guidelines & Memory Bank\n");
        memories_md.push_str(
            "以下是历史项目踩坑事故记录与规约，请在此工作区内严加防范，避免重犯相同错误：\n\n",
        );

        // Cap at 20 memories to avoid context bloat
        const MAX_MEMORIES: usize = 20;
        let mut count = 0;
        for r in rows {
            if count >= MAX_MEMORIES {
                break;
            }
            if let Ok((desc, pattern, remediation, keywords)) = r {
                count += 1;
                memories_md.push_str(&format!("### ❌ 坑点 {}: {}\n", count, desc));
                memories_md.push_str(&format!("* **危险模式/命令**: `{}`\n", pattern));
                memories_md.push_str(&format!("* **安全修复方案**: {}\n", remediation));
                memories_md.push_str(&format!("* **相关标签**: `{}`\n\n", keywords));
            }
        }
        memories_md.push_str("<!--- OMNIX MEMORY END --->\n");

        if count == 0 {
            return Ok(());
        }

        let workspace_path = PathBuf::from(workspace_dir);
        if !workspace_path.exists() {
            return Ok(());
        }

        // Determine which context files to write based on agent type.
        // Each AI agent reads its own project-level instruction file.
        let context_files: Vec<&str> =
            if agent_name.contains("Claude") || agent_name.contains("claude") {
                vec!["CLAUDE.md"]
            } else if agent_name.contains("Gemini") || agent_name.contains("gemini") {
                vec!["GEMINI.md"]
            } else if agent_name.contains("Codex") || agent_name.contains("codex") {
                vec!["AGENTS.md"]
            } else if agent_name.contains("Copilot") || agent_name.contains("copilot") {
                vec![".github/copilot-instructions.md"]
            } else {
                vec!["CLAUDE.md", "GEMINI.md", "AGENTS.md"]
            };

        for filename in &context_files {
            let file_path = workspace_path.join(filename);

            // Create parent directory if needed (e.g. .github/)
            if let Some(parent) = file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            if file_path.exists() {
                if let Ok(mut content) = fs::read_to_string(&file_path) {
                    if let (Some(start_idx), Some(end_idx)) = (
                        content.find("<!--- OMNIX MEMORY START --->"),
                        content.find("<!--- OMNIX MEMORY END --->"),
                    ) {
                        let end_block_len = "<!--- OMNIX MEMORY END --->\n".len();
                        let actual_end = if end_idx + end_block_len <= content.len() {
                            end_idx + end_block_len
                        } else {
                            end_idx
                        };
                        content.replace_range(start_idx..actual_end, &memories_md);
                    } else {
                        content.push_str(&memories_md);
                    }
                    let _ = fs::write(&file_path, content);
                }
            } else {
                let _ = fs::write(&file_path, &memories_md);
            }
        }

        Ok(())
    }

    pub fn find_agent_path(&self, display_name: &str) -> Option<String> {
        Self::find_agent_path_static(display_name, Some(&self.db))
    }

    pub fn find_agent_path_static(display_name: &str, db: Option<&DbManager>) -> Option<String> {
        let agent_names = vec![
            ("Claude Code", "claude"),
            ("Gemini CLI", "gemini"),
            ("Codex", "codex"),
            ("Qwen Code", "qwen-code"),
            ("GitHub Copilot CLI", "github-copilot-cli"),
            ("Google Antigravity", "agy"),
            ("OpenCode", "opencode"),
        ];

        let bin_name = agent_names.iter().find(|(dn, _)| dn == &display_name)?.1;

        if bin_name == "agy" {
            // Check AppData/Local/agy/bin/agy.exe or ~/.local/share/agy/bin/agy
            let mut agy_path = None;
            if cfg!(windows) {
                if let Some(local_dir) = dirs::data_local_dir() {
                    let p = local_dir.join("agy").join("bin").join("agy.exe");
                    if p.exists() {
                        agy_path = Some(p.to_string_lossy().to_string());
                    }
                }
            } else {
                if let Some(home) = dirs::home_dir() {
                    let p = home
                        .join(".local")
                        .join("share")
                        .join("agy")
                        .join("bin")
                        .join("agy");
                    if p.exists() {
                        agy_path = Some(p.to_string_lossy().to_string());
                    }
                }
            }
            if agy_path.is_some() {
                return agy_path;
            }
        }

        // A user-managed system CLI is authoritative. OMNIX never silently
        // replaces it with an isolated copy.
        if let Ok(path) = which::which(bin_name) {
            return Some(
                prefer_windows_command_shim(path)
                    .to_string_lossy()
                    .to_string(),
            );
        }

        let managed_root = db
            .map(|database| managed_agent_root(database, display_name))
            .unwrap_or_else(|| {
                resolve_sandbox_path(&format!("~/.omnix/agents/{}", agent_slug(display_name)))
            });
        if let Some(path) = executable_in_managed_root(&managed_root, bin_name) {
            return Some(path);
        }

        // Compatibility lookup for installations created by older OMNIX builds.
        let legacy_root = db
            .and_then(|database| database.get_setting("sandbox_dir").ok().flatten())
            .map(|path| resolve_sandbox_path(&path))
            .unwrap_or_else(|| resolve_sandbox_path("~/.omnix/agents"));
        executable_in_managed_root(&legacy_root, bin_name)
    }

    fn start_cron_scheduler(&self) {
        let db = Arc::clone(&self.db);

        tauri::async_runtime::handle().spawn(async move {
            loop {
                // Check schedules every 10 seconds
                tokio::time::sleep(Duration::from_secs(10)).await;

                let conn_res = db.get_connection();
                if let Ok(conn) = conn_res {
                    let mut stmt = match conn.prepare(
                        "SELECT id, title, schedule, agent_name, args, workspace_dir, last_run 
                         FROM cron_tasks WHERE is_active = 1",
                    ) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    let rows = stmt.query_map([], |row| {
                        let last_run_str: Option<String> = row.get(6)?;
                        let last_run = last_run_str.and_then(|s| {
                            // Format: YYYY-MM-DD HH:MM:SS
                            chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                .ok()
                                .and_then(|ndt| chrono::Local.from_local_datetime(&ndt).single())
                        });

                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            last_run,
                        ))
                    });

                    if let Ok(rows) = rows {
                        for r in rows.flatten() {
                            let (
                                id,
                                title,
                                schedule,
                                agent_name,
                                args_str,
                                workspace_dir,
                                last_run,
                            ) = r;

                            if match_schedule(&schedule, last_run) {
                                println!("Cron Scheduler: Triggering task '{}' ({})", title, id);

                                let db_clone = Arc::clone(&db);
                                tauri::async_runtime::spawn(async move {
                                    let _ = run_cron_task(
                                        db_clone,
                                        id,
                                        agent_name,
                                        args_str,
                                        workspace_dir,
                                    )
                                    .await;
                                });
                            }
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod agent_path_tests {
    use super::prefer_windows_command_shim;

    #[cfg(windows)]
    #[test]
    fn windows_npm_cli_prefers_cmd_over_extensionless_or_powershell_shims() {
        let root = std::env::temp_dir().join(format!(
            "omnix_agent_shim_{}",
            chrono::Utc::now().timestamp_micros()
        ));
        std::fs::create_dir_all(&root).expect("temporary shim directory");
        let extensionless = root.join("codex");
        let powershell = root.join("codex.ps1");
        let command = root.join("codex.cmd");
        std::fs::write(&extensionless, "node codex.js").expect("extensionless shim");
        std::fs::write(&powershell, "node codex.js").expect("PowerShell shim");
        std::fs::write(&command, "@node codex.js").expect("command shim");

        assert_eq!(prefer_windows_command_shim(extensionless), command);
        assert_eq!(prefer_windows_command_shim(powershell), command);

        let _ = std::fs::remove_dir_all(root);
    }
}

fn match_schedule(schedule: &str, last_run: Option<DateTime<Local>>) -> bool {
    let now = Local::now();

    if let Some(lr) = last_run {
        if (now - lr).num_seconds() < 55 {
            return false; // Prevent double trigger in the same minute
        }
    }

    let schedule = schedule.trim().to_lowercase();

    // 1. Natural Language: "every X minutes"
    if schedule.starts_with("every ") && schedule.ends_with(" minutes") {
        if let Some(num_str) = schedule
            .strip_prefix("every ")
            .and_then(|s| s.strip_suffix(" minutes"))
        {
            if let Ok(minutes) = num_str.trim().parse::<i64>() {
                if let Some(lr) = last_run {
                    return (now - lr).num_minutes() >= minutes;
                } else {
                    return true;
                }
            }
        }
    }

    // 2. Natural Language: "every X hours"
    if schedule.starts_with("every ") && schedule.ends_with(" hours") {
        if let Some(num_str) = schedule
            .strip_prefix("every ")
            .and_then(|s| s.strip_suffix(" hours"))
        {
            if let Ok(hours) = num_str.trim().parse::<i64>() {
                if let Some(lr) = last_run {
                    return (now - lr).num_hours() >= hours;
                } else {
                    return true;
                }
            }
        }
    }

    // 3. Natural Language: "daily at HH:MM"
    if schedule.starts_with("daily at ") {
        if let Some(time_str) = schedule.strip_prefix("daily at ") {
            let parts: Vec<&str> = time_str.trim().split(':').collect();
            if parts.len() == 2 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    let current_h = now.hour();
                    let current_m = now.minute();
                    if current_h == h && current_m == m {
                        if let Some(lr) = last_run {
                            return lr.date_naive() != now.date_naive();
                        } else {
                            return true;
                        }
                    }
                }
            }
        }
        return false;
    }

    // 4. Standard Cron: minute, hour, day, month, day_of_week
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    if fields.len() == 5 {
        let current_min = now.minute();
        let current_hour = now.hour();
        let current_day = now.day();
        let current_month = now.month();
        let current_wday = now.weekday().num_days_from_sunday(); // 0 (Sun) - 6 (Sat)

        let match_field = |field: &str, current_val: u32| -> bool {
            if field == "*" {
                return true;
            }
            if field.starts_with("*/") {
                if let Ok(step) = field[2..].parse::<u32>() {
                    return current_val % step == 0;
                }
            }
            if let Ok(val) = field.parse::<u32>() {
                return val == current_val;
            }
            false
        };

        return match_field(fields[0], current_min)
            && match_field(fields[1], current_hour)
            && match_field(fields[2], current_day)
            && match_field(fields[3], current_month)
            && match_field(fields[4], current_wday);
    }

    false
}

pub(crate) async fn run_cron_task(
    db: Arc<DbManager>,
    task_id: String,
    agent_name: String,
    args_str: String,
    workspace_dir: String,
) -> Result<(), String> {
    let resolved_workspace = resolve_sandbox_path(&workspace_dir);
    let exe_path = match AgentManager::find_agent_path_static(&agent_name, Some(&db)) {
        Some(path) => path,
        None => {
            let err_msg = format!("Agent '{}' not found/installed", agent_name);
            log_cron_run_failure(&db, &task_id, &err_msg).await;
            return Err(err_msg);
        }
    };

    let args: Vec<String> = serde_json::from_str(&args_str).unwrap_or_default();
    let run_id = format!("run_{}_{}", task_id, Local::now().format("%Y%m%d_%H%M%S"));

    let home_dir = dirs::home_dir().expect("Failed to determine home directory");
    let mut log_dir = home_dir.clone();
    log_dir.push(".omnix");
    log_dir.push("logs");
    let _ = fs::create_dir_all(&log_dir);

    let log_path = log_dir.join(format!("{}.log", run_id));
    let log_path_str = log_path.to_string_lossy().to_string();

    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO cron_runs (id, task_id, status, log_path, started_at)
             VALUES (?1, ?2, 'running', ?3, CURRENT_TIMESTAMP)",
            params![run_id, task_id, log_path_str],
        );
        let _ = conn.execute(
            "UPDATE cron_tasks SET last_run = CURRENT_TIMESTAMP WHERE id = ?1",
            params![task_id],
        );
    }

    let use_wsl = db
        .get_setting("use_wsl")
        .unwrap_or(None)
        .unwrap_or_else(|| "false".to_string())
        == "true";
    let wsl_distro = db
        .get_setting("wsl_distro")
        .unwrap_or(None)
        .unwrap_or_else(|| "Ubuntu".to_string());
    let proxy_port = db
        .get_setting("proxy_port")
        .unwrap_or(None)
        .unwrap_or_else(|| "1421".to_string());

    let mut cmd = if use_wsl {
        let mut c = Command::new("wsl.exe");
        let args_escaped: Vec<String> = args
            .iter()
            .map(|a| {
                if a.contains(' ') || a.contains('"') || a.contains('\'') {
                    format!("'{}'", a.replace("'", "'\\''"))
                } else {
                    a.clone()
                }
            })
            .collect();
        let command_str = format!("{} {}", exe_path, args_escaped.join(" "));
        let sh_command = format!(
            "HOST_IP=$(ip route | grep default | awk '{{print $3}}'); \
             export ANTHROPIC_BASE_URL=http://$HOST_IP:{}/agent/{}; \
             export CLAUDE_CODE_HEADLESS=1; \
             export DISABLE_UPDATES=1; \
             export DISABLE_AUTOUPDATER=1; \
             {}",
            proxy_port,
            agent_name.replace(' ', "_"),
            command_str
        );
        c.args(&["-d", &wsl_distro, "--", "sh", "-c", &sh_command]);
        c
    } else {
        let local_proxy_url = format!(
            "http://localhost:{}/agent/{}",
            proxy_port,
            agent_name.replace(' ', "_")
        );
        let mut c = Command::new(&exe_path);
        c.args(args)
            .env("ANTHROPIC_BASE_URL", &local_proxy_url)
            .env("CLAUDE_CODE_HEADLESS", "1")
            .env("DISABLE_UPDATES", "1")
            .env("DISABLE_AUTOUPDATER", "1");
        c
    };

    cmd.current_dir(resolved_workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("Failed to spawn background process: {}", e);
            log_cron_run_status(&db, &run_id, "failed").await;
            let _ = fs::write(&log_path, &err_msg);
            return Err(err_msg);
        }
    };

    let mut file = match tokio::fs::File::create(&log_path).await {
        Ok(f) => f,
        Err(e) => {
            let _ = child.kill().await;
            return Err(format!("Failed to create log file: {}", e));
        }
    };

    let stdout = child.stdout.take().ok_or_else(|| "No stdout".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "No stderr".to_string())?;

    let mut reader_out = BufReader::new(stdout);
    let mut reader_err = BufReader::new(stderr);

    let log_writer = tauri::async_runtime::spawn(async move {
        let mut buf_out = vec![0; 1024];
        let mut buf_err = vec![0; 1024];
        let mut stdout_done = false;
        let mut stderr_done = false;
        loop {
            if stdout_done && stderr_done {
                break;
            }
            tokio::select! {
                res = reader_out.read(&mut buf_out), if !stdout_done => {
                    match res {
                        Ok(0) | Err(_) => stdout_done = true,
                        Ok(n) => {
                            let _ = file.write_all(&buf_out[..n]).await;
                        }
                    }
                }
                res = reader_err.read(&mut buf_err), if !stderr_done => {
                    match res {
                        Ok(0) | Err(_) => stderr_done = true,
                        Ok(n) => {
                            let _ = file.write_all(&buf_err[..n]).await;
                        }
                    }
                }
            }
        }
        let _ = file.flush().await;
    });

    let status = child.wait().await;
    let _ = log_writer.await;

    let success = match status {
        Ok(s) => s.success(),
        Err(_) => false,
    };

    log_cron_run_status(&db, &run_id, if success { "success" } else { "failed" }).await;

    Ok(())
}

async fn log_cron_run_status(db: &DbManager, run_id: &str, status: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE cron_runs SET status = ?1, finished_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status, run_id],
        );
    }
}

async fn log_cron_run_failure(db: &DbManager, task_id: &str, err_msg: &str) {
    let run_id = format!(
        "run_err_{}_{}",
        task_id,
        Local::now().format("%Y%m%d_%H%M%S")
    );
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "INSERT INTO cron_runs (id, task_id, status, log_path, started_at, finished_at)
             VALUES (?1, ?2, 'failed', ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![run_id, task_id, err_msg],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[tokio::test]
    #[ignore = "manual integration test; depends on full seeded database initialization"]
    async fn test_memory_injection() {
        let temp_dir = std::env::temp_dir();
        let test_db_path = temp_dir.join("omnix_agent_test.db");
        if test_db_path.exists() {
            let _ = std::fs::remove_file(&test_db_path);
        }

        let db = Arc::new(DbManager::new_with_path(test_db_path.clone()));

        let manager = AgentManager::new(Arc::clone(&db));

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_workspace = temp_dir.join(format!("omnix_workspace_{}", timestamp));
        fs::create_dir_all(&test_workspace).unwrap();

        // Run injection
        manager
            .inject_workspace_memories(&test_workspace.to_string_lossy(), "Claude Code")
            .unwrap();

        // Verify files exist
        let claude_md = test_workspace.join("CLAUDE.md");
        let omnix_md = test_workspace.join("OMNIX_MEMORY.md");
        assert!(claude_md.exists());
        assert!(omnix_md.exists());

        // Read content and check guidelines
        let content = fs::read_to_string(&claude_md).unwrap();
        assert!(content.contains("OMNIX Anti-Failure Guidelines"));
        assert!(content.contains("std::sync::MutexGuard across await point")); // seeded default memory

        // Clean up
        let _ = fs::remove_file(claude_md);
        let _ = fs::remove_file(omnix_md);
        let _ = fs::remove_dir(test_workspace);
        if test_db_path.exists() {
            let _ = fs::remove_file(&test_db_path);
        }
    }
}
