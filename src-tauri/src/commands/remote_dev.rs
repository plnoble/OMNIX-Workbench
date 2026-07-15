//! Remote Dev (Labs) — 用家里的 Linux 服务器补足开发算力。
//!
//! 分三层（对应 P0/P1/P2）：
//! - P0 远程模型主机：连通性/延迟/模型列表测试（Ollama/vLLM 等 OpenAI 兼容端点），
//!   测通后去模型中心添加即可让全软件用上远端显卡。
//! - P1 SSH 执行：主机管理 + 运行测试台。用系统 `ssh.exe`（继承 ~/.ssh/config、
//!   密钥、known_hosts），`-R` 反向转发把本机网关带到远端，Claude 会话在远端
//!   跑但技能注入/模型路由/多账号全部生效。
//! - P2 远端管理：硬件探测（nvidia-smi）、远端 agent CLI 检测与安装。
//!
//! Labs 定位：独立测试台，不接主对话运行时；验证稳定后再转正。

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::db::DbManager;
use crate::proc::NoWindow;

// ── 主机模型 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshHost {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 私钥路径；留空则交给系统 ssh 配置（~/.ssh/config / ssh-agent）。
    pub key_path: String,
    pub default_workdir: String,
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ssh_hosts (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            host TEXT NOT NULL,
            port INTEGER NOT NULL DEFAULT 22,
            user TEXT NOT NULL DEFAULT '',
            key_path TEXT NOT NULL DEFAULT '',
            default_workdir TEXT NOT NULL DEFAULT '',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn load_host(db: &DbManager, id: &str) -> Result<SshHost, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, name, host, port, user, key_path, default_workdir FROM ssh_hosts WHERE id = ?1",
        params![id],
        |r| {
            Ok(SshHost {
                id: r.get(0)?,
                name: r.get(1)?,
                host: r.get(2)?,
                port: r.get::<_, i64>(3)? as u16,
                user: r.get(4)?,
                key_path: r.get(5)?,
                default_workdir: r.get(6)?,
            })
        },
    )
    .map_err(|_| "主机不存在".to_string())
}

/// Base ssh args: non-interactive, fail fast, first-connect auto-accept.
fn ssh_args(h: &SshHost) -> Vec<String> {
    let mut a = vec![
        "-o".into(), "BatchMode=yes".into(),
        "-o".into(), "ConnectTimeout=10".into(),
        "-o".into(), "StrictHostKeyChecking=accept-new".into(),
        "-p".into(), h.port.to_string(),
    ];
    if !h.key_path.trim().is_empty() {
        a.push("-i".into());
        a.push(h.key_path.trim().into());
    }
    a.push(if h.user.trim().is_empty() {
        h.host.clone()
    } else {
        format!("{}@{}", h.user.trim(), h.host)
    });
    a
}

/// Run one remote command (non-interactive) and capture stdout/stderr.
async fn ssh_capture(h: &SshHost, remote_cmd: &str) -> Result<(String, String, bool), String> {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.args(ssh_args(h)).arg("--").arg("sh").arg("-lc").arg(remote_cmd);
    cmd.no_window();
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("ssh 启动失败（Windows 需已启用 OpenSSH 客户端）: {e}"))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    ))
}

// ── 主机 CRUD ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_ssh_hosts(db: State<'_, Arc<DbManager>>) -> Result<Vec<SshHost>, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, host, port, user, key_path, default_workdir FROM ssh_hosts ORDER BY created_at")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SshHost {
                id: r.get(0)?,
                name: r.get(1)?,
                host: r.get(2)?,
                port: r.get::<_, i64>(3)? as u16,
                user: r.get(4)?,
                key_path: r.get(5)?,
                default_workdir: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn save_ssh_host(mut host: SshHost, db: State<'_, Arc<DbManager>>) -> Result<SshHost, String> {
    ensure_table(&db)?;
    if host.host.trim().is_empty() {
        return Err("主机地址不能为空".into());
    }
    if host.id.trim().is_empty() {
        host.id = format!("sshh_{}", chrono::Utc::now().timestamp_micros());
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO ssh_hosts (id, name, host, port, user, key_path, default_workdir)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, host=excluded.host,
            port=excluded.port, user=excluded.user, key_path=excluded.key_path,
            default_workdir=excluded.default_workdir",
        params![host.id, host.name, host.host, host.port as i64, host.user, host.key_path, host.default_workdir],
    )
    .map_err(|e| e.to_string())?;
    Ok(host)
}

#[tauri::command]
pub fn delete_ssh_host(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM ssh_hosts WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── P1: 连接测试 / P2: 探测与远端 agent ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub uname: String,
    pub error: String,
}

#[tauri::command]
pub async fn test_ssh_host(id: String, db: State<'_, Arc<DbManager>>) -> Result<SshTestResult, String> {
    let h = load_host(&db, &id)?;
    let t = std::time::Instant::now();
    let (out, err, ok) = ssh_capture(&h, "uname -a").await?;
    Ok(SshTestResult {
        ok,
        latency_ms: t.elapsed().as_millis() as u64,
        uname: out.trim().to_string(),
        error: if ok { String::new() } else { err.trim().to_string() },
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHardware {
    pub gpu: String,
    pub ram_mb: u64,
    pub cpu_cores: u32,
}

#[tauri::command]
pub async fn probe_remote_hardware(id: String, db: State<'_, Arc<DbManager>>) -> Result<RemoteHardware, String> {
    let h = load_host(&db, &id)?;
    let script = "nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null | head -1; echo '---'; free -m 2>/dev/null | awk '/^Mem:/{print $2}'; echo '---'; nproc 2>/dev/null";
    let (out, err, ok) = ssh_capture(&h, script).await?;
    if !ok {
        return Err(format!("探测失败: {}", err.trim()));
    }
    let parts: Vec<&str> = out.split("---").collect();
    Ok(RemoteHardware {
        gpu: parts
            .first()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "（未检测到 NVIDIA GPU）".into()),
        ram_mb: parts.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(0),
        cpu_cores: parts.get(2).and_then(|s| s.trim().parse().ok()).unwrap_or(0),
    })
}

/// (display, bin, npm package) — the remotely installable coding CLIs.
const REMOTE_AGENTS: &[(&str, &str, &str)] = &[
    ("Claude Code", "claude", "@anthropic-ai/claude-code"),
    ("Codex", "codex", "@openai/codex"),
    ("Gemini CLI", "gemini", "@google/gemini-cli"),
    ("OpenCode", "opencode", "opencode-ai"),
    ("Grok Build", "grok", "@xai-official/grok"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAgentStatus {
    pub agent: String,
    pub bin: String,
    pub installed: bool,
    pub path: String,
    pub version: String,
}

#[tauri::command]
pub async fn detect_remote_agents(id: String, db: State<'_, Arc<DbManager>>) -> Result<Vec<RemoteAgentStatus>, String> {
    let h = load_host(&db, &id)?;
    // One round-trip: for each bin print `bin|path|version` (or `bin||`).
    let script = REMOTE_AGENTS
        .iter()
        .map(|(_, bin, _)| {
            format!(
                "p=$(command -v {bin} 2>/dev/null); if [ -n \"$p\" ]; then v=$({bin} --version 2>/dev/null | head -1); echo '{bin}|'\"$p\"'|'\"$v\"; else echo '{bin}||'; fi"
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let (out, err, ok) = ssh_capture(&h, &script).await?;
    if !ok {
        return Err(format!("检测失败: {}", err.trim()));
    }
    let mut map: HashMap<&str, (String, String)> = HashMap::new();
    for line in out.lines() {
        let mut it = line.splitn(3, '|');
        if let (Some(bin), Some(path), Some(ver)) = (it.next(), it.next(), it.next()) {
            map.insert(
                REMOTE_AGENTS.iter().find(|(_, b, _)| *b == bin).map(|(_, b, _)| *b).unwrap_or(""),
                (path.trim().to_string(), ver.trim().to_string()),
            );
            let _ = bin;
        }
    }
    Ok(REMOTE_AGENTS
        .iter()
        .map(|(display, bin, _)| {
            let (path, version) = map.get(bin).cloned().unwrap_or_default();
            RemoteAgentStatus {
                agent: (*display).to_string(),
                bin: (*bin).to_string(),
                installed: !path.is_empty(),
                path,
                version,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn install_remote_agent(id: String, agent: String, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    let h = load_host(&db, &id)?;
    let (_, _, pkg) = REMOTE_AGENTS
        .iter()
        .find(|(d, _, _)| *d == agent)
        .ok_or_else(|| format!("未知 agent: {agent}"))?;
    let (out, err, ok) = ssh_capture(&h, &format!("npm install -g {pkg} 2>&1 | tail -3")).await?;
    if ok {
        Ok(out.trim().to_string())
    } else {
        Err(format!("远端安装失败（远端需已装 Node/npm）: {} {}", out.trim(), err.trim()))
    }
}

// ── P0: 远程模型主机连通性 ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteModelHostTest {
    pub ok: bool,
    pub latency_ms: u64,
    pub models: Vec<String>,
    pub error: String,
}

#[tauri::command]
pub async fn test_remote_model_host(url: String) -> Result<RemoteModelHostTest, String> {
    let base = url.trim().trim_end_matches('/').to_string();
    if !base.starts_with("http") {
        return Err("请输入完整地址，例如 http://192.168.1.10:11434/v1".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    match client.get(format!("{base}/models")).send().await {
        Ok(resp) if resp.status().is_success() => {
            let latency = t.elapsed().as_millis() as u64;
            let json: serde_json::Value = resp.json().await.unwrap_or_default();
            let models = json["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m["id"].as_str().map(String::from))
                        .take(20)
                        .collect()
                })
                .unwrap_or_default();
            Ok(RemoteModelHostTest { ok: true, latency_ms: latency, models, error: String::new() })
        }
        Ok(resp) => Ok(RemoteModelHostTest {
            ok: false,
            latency_ms: t.elapsed().as_millis() as u64,
            models: vec![],
            error: format!("HTTP {}", resp.status()),
        }),
        Err(e) => Ok(RemoteModelHostTest {
            ok: false,
            latency_ms: t.elapsed().as_millis() as u64,
            models: vec![],
            error: e.to_string(),
        }),
    }
}

// ── P1: 远程运行测试台 ──────────────────────────────────────────────────────

/// 网关在远端的回连端口（`ssh -R` 反向转发到本机 1421）。
const REMOTE_GATEWAY_PORT: u16 = 18421;

fn running_map() -> &'static Mutex<HashMap<String, tokio::process::Child>> {
    static MAP: OnceLock<Mutex<HashMap<String, tokio::process::Child>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteRunStarted {
    pub run_id: String,
}

/// Start a headless agent turn on the remote host. For Claude Code the local
/// gateway is reverse-forwarded (`-R`), so官方技能注入/模型路由在远端同样生效;
/// Codex/Grok use their own credentials already configured on the server.
#[tauri::command]
pub async fn start_remote_run(
    host_id: String,
    agent: String,
    workdir: String,
    prompt: String,
    use_gateway: bool,
    app: AppHandle,
    db: State<'_, Arc<DbManager>>,
) -> Result<RemoteRunStarted, String> {
    let h = load_host(&db, &host_id)?;
    if prompt.trim().is_empty() {
        return Err("请输入要执行的任务".into());
    }
    let q = sh_quote(prompt.trim());
    let agent_cmd = match agent.as_str() {
        "Claude Code" => format!("claude -p {q} --output-format text"),
        "Codex" => format!("codex exec {q}"),
        "Grok Build" => format!("grok -p {q} --no-auto-update"),
        other => return Err(format!("运行测试台暂不支持 {other}")),
    };
    let cd = if workdir.trim().is_empty() {
        String::new()
    } else {
        format!("cd {} && ", sh_quote(workdir.trim()))
    };
    // Claude 回连本机网关：技能正式池注入/模型路由/用量统计全部生效。
    let env = if use_gateway && agent == "Claude Code" {
        format!(
            "export ANTHROPIC_BASE_URL=http://127.0.0.1:{REMOTE_GATEWAY_PORT}/agent/Claude_Code; \
             export ANTHROPIC_API_KEY=dummy-key-for-omnix; export DISABLE_AUTOUPDATER=1; "
        )
    } else {
        String::new()
    };
    let script = format!("{cd}{env}{agent_cmd} 2>&1");

    let mut cmd = tokio::process::Command::new("ssh");
    cmd.args(ssh_args(&h));
    if use_gateway {
        cmd.arg("-R").arg(format!("{REMOTE_GATEWAY_PORT}:127.0.0.1:1421"));
    }
    cmd.arg("--").arg("sh").arg("-lc").arg(&script);
    cmd.no_window().stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("ssh 启动失败: {e}"))?;
    let run_id = format!("rr_{}", chrono::Utc::now().timestamp_micros());
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    running_map().lock().unwrap().insert(run_id.clone(), child);

    for (stream, is_err) in [(stdout, false)] {
        let app2 = app.clone();
        let rid = run_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app2.emit("remote-run-output", serde_json::json!({"run_id": rid, "line": line, "stderr": is_err}));
            }
        });
    }
    {
        let app2 = app.clone();
        let rid = run_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app2.emit("remote-run-output", serde_json::json!({"run_id": rid, "line": line, "stderr": true}));
            }
        });
    }
    // Waiter: reap the child and emit completion.
    {
        let app2 = app.clone();
        let rid = run_id.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let done = {
                    let mut map = running_map().lock().unwrap();
                    match map.get_mut(&rid) {
                        Some(child) => match child.try_wait() {
                            Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
                            Ok(None) => None,
                            Err(_) => Some(-1),
                        },
                        None => Some(-2), // stopped by user
                    }
                };
                if let Some(code) = done {
                    running_map().lock().unwrap().remove(&rid);
                    let _ = app2.emit("remote-run-done", serde_json::json!({"run_id": rid, "code": code}));
                    break;
                }
            }
        });
    }
    Ok(RemoteRunStarted { run_id })
}

#[tauri::command]
pub async fn stop_remote_run(run_id: String) -> Result<(), String> {
    let child = running_map().lock().unwrap().remove(&run_id);
    if let Some(mut child) = child {
        let _ = child.kill().await;
    }
    Ok(())
}
