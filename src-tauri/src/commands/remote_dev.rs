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

/// Base ssh args: non-interactive, fail fast, known host key required.
///
/// 以前这里是 `StrictHostKeyChecking=accept-new`：第一次连接自动接受对方的主机
/// 密钥。省一步确认，代价是**首连没有任何中间人保护**——有人在你和服务器之间
/// 应答，ssh 会安静地把他的密钥记下来，之后每次都「验证通过」。
///
/// 改成 `yes`：未知主机直接拒。用户要先自己确认指纹并 `ssh-keyscan` 入
/// known_hosts（错误信息里给了命令），之后 OMNIX 才连得上。多一步，但那一步
/// 正是信任的建立点，不该由程序替他跳过。
fn ssh_args(h: &SshHost) -> Result<Vec<String>, String> {
    // Validate destination up front so a host like `-oProxyCommand=…` never
    // reaches the argv as an OpenSSH option.
    let _ = ssh_destination(h)?;
    let mut a = vec![
        "-o".into(), "BatchMode=yes".into(),
        "-o".into(), "ConnectTimeout=10".into(),
        "-o".into(), "StrictHostKeyChecking=yes".into(),
        "-p".into(), h.port.to_string(),
    ];
    if !h.key_path.trim().is_empty() {
        a.push("-i".into());
        a.push(h.key_path.trim().into());
    }
    Ok(a)
}

fn ssh_destination(h: &SshHost) -> Result<String, String> {
    let host = h.host.trim();
    let user = h.user.trim();
    if host.is_empty() {
        return Err("主机名不能为空".into());
    }
    if host.starts_with('-') || user.starts_with('-') {
        return Err("主机名和用户名不能以 '-' 开头（会被 ssh 当成选项）".into());
    }
    if host.contains([' ', '@', '\n', '\r']) || user.contains([' ', '@', '\n', '\r']) {
        return Err("主机名和用户名不能包含空格或 @".into());
    }
    Ok(if user.is_empty() {
        host.to_string()
    } else {
        format!("{user}@{host}")
    })
}

/// Run one remote command (non-interactive) and capture stdout/stderr.
async fn ssh_capture(h: &SshHost, remote_cmd: &str) -> Result<(String, String, bool), String> {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.args(ssh_args(h)?)
        .arg("--")
        .arg(ssh_destination(h)?)
        .arg("sh")
        .arg("-lc")
        .arg(remote_cmd);
    cmd.no_window();
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("ssh 启动失败（Windows 需已启用 OpenSSH 客户端）: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok((
        String::from_utf8_lossy(&out.stdout).to_string(),
        augment_host_key_error(&stderr, h),
        out.status.success(),
    ))
}

/// 主机密钥没在 known_hosts 里时，ssh 的原话是一句英文 + 一句「Host key
/// verification failed.」，看不出该做什么。既然是我们把默认值收紧成 `yes` 的，
/// 就该把下一步一并给出来。
fn augment_host_key_error(stderr: &str, h: &SshHost) -> String {
    let unknown = stderr.contains("Host key verification failed")
        || stderr.contains("No RSA host key is known")
        || stderr.contains("no matching host key")
        || (stderr.contains("Host key") && stderr.contains("not known"));
    if !unknown {
        return stderr.to_string();
    }
    format!(
        "{stderr}\n\
         ── OMNIX 提示 ──\n\
         这台主机的密钥不在 known_hosts 里，已拒绝连接（防中间人）。\n\
         确认指纹无误后执行一次，之后就能连：\n\
         ssh-keyscan -p {port} {host} >> ~/.ssh/known_hosts\n\
         指纹可以在服务器上用 `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` 查。",
        port = h.port,
        host = h.host,
    )
}

// ── 主机 CRUD ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_ssh_hosts(db: State<'_, Arc<DbManager>>) -> Result<Vec<SshHost>, String> {
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

/// 这个功能的用途是**探测局域网里的 Ollama / vLLM**，所以不能套用
/// `search.rs::guard_public_url`（那条只放行公网）——RFC1918 必须放行。
///
/// 但也不能什么都放行：它接受任意 URL、把响应原样回渲染进程，等于一个
/// IPC 版 SSRF。至少要挡住三类：
/// - **回环**：本机 1421 网关对回环是免令牌的，探到就等于绕过鉴权；
/// - **链路本地**：`169.254.169.254` 是各家云的元数据端点（临时凭据）；
/// - **未指定地址**：`0.0.0.0` / `::` 在部分栈上等价于回环。
///
/// 主机名要**解析之后**再判，否则一个指向 127.0.0.1 的域名就能长驱直入。
/// 和 `guard_public_url` 一样，解析与连接之间的 DNS rebinding 窗口仍在，
/// 堵死它要接管连接层——那是另一层机器，这里先把大路封掉，窗口写在注释里。
fn guard_lan_url(base: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(base).map_err(|e| format!("地址解析失败：{e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("只支持 http/https，收到 {}", parsed.scheme()));
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() {
        return Err("地址里没有主机名".into());
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Err("拒绝探测本机地址：本机网关对回环免鉴权，这里放行等于开后门".into());
    }

    let deny = |ip: std::net::IpAddr| -> Option<String> {
        let v4 = match ip {
            std::net::IpAddr::V4(v4) => Some(v4),
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() || v6.is_unspecified() {
                    return Some(format!("拒绝探测本机地址：{ip}"));
                }
                if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    return Some(format!("拒绝探测链路本地地址：{ip}"));
                }
                v6.to_ipv4_mapped()
            }
        };
        let v4 = v4?;
        if v4.is_loopback() || v4.is_unspecified() {
            return Some(format!("拒绝探测本机地址：{v4}"));
        }
        if v4.is_link_local() {
            return Some(format!("拒绝探测链路本地地址（云元数据端点）：{v4}"));
        }
        None
    };

    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        return match deny(ip) {
            Some(message) => Err(message),
            None => Ok(()),
        };
    }

    let port = parsed.port_or_known_default().unwrap_or(80);
    let resolved: Vec<std::net::IpAddr> =
        std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
            .map_err(|e| format!("无法解析主机名 {host}：{e}"))?
            .map(|addr| addr.ip())
            .collect();
    if resolved.is_empty() {
        return Err(format!("主机名 {host} 没有解析出任何地址"));
    }
    for ip in resolved {
        if let Some(message) = deny(ip) {
            return Err(format!("{message}（{host} 解析到这里）"));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn test_remote_model_host(url: String) -> Result<RemoteModelHostTest, String> {
    let base = url.trim().trim_end_matches('/').to_string();
    if !base.starts_with("http") {
        return Err("请输入完整地址，例如 http://192.168.1.10:11434/v1".into());
    }
    guard_lan_url(&base)?;
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
    cmd.args(ssh_args(&h)?);
    if use_gateway {
        cmd.arg("-R").arg(format!("{REMOTE_GATEWAY_PORT}:127.0.0.1:1421"));
    }
    cmd.arg("--")
        .arg(ssh_destination(&h)?)
        .arg("sh")
        .arg("-lc")
        .arg(&script);
    cmd.no_window().stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("ssh 启动失败: {e}"))?;
    let run_id = format!("rr_{}", chrono::Utc::now().timestamp_micros());
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    running_map().lock().unwrap().insert(run_id.clone(), child);

    {
        let (stream, is_err) = (stdout, false);
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

#[cfg(test)]
mod host_key_tests {
    use super::*;

    fn host() -> SshHost {
        SshHost {
            id: "h1".into(),
            name: "家里那台".into(),
            host: "192.0.2.10".into(),
            port: 2222,
            user: "me".into(),
            key_path: String::new(),
            default_workdir: String::new(),
        }
    }

    /// 首连不能自动接受主机密钥——那正是中间人能插进来的那一步。
    #[test]
    fn unknown_hosts_are_refused_not_auto_accepted() {
        let args = ssh_args(&host()).expect("valid fixture host").join(" ");
        assert!(
            args.contains("StrictHostKeyChecking=yes"),
            "首连必须拒绝未知主机：{args}"
        );
        assert!(
            !args.contains("accept-new"),
            "accept-new 会安静地记下中间人的密钥：{args}"
        );
    }

    /// 拒绝之后要告诉用户怎么办，否则「连不上」就成了死路。
    #[test]
    fn the_refusal_explains_the_next_step() {
        let augmented = augment_host_key_error("Host key verification failed.", &host());
        assert!(augmented.contains("ssh-keyscan"), "{augmented}");
        assert!(augmented.contains("2222"), "命令里要带上真实端口：{augmented}");
        assert!(augmented.contains("192.0.2.10"), "{augmented}");
    }

    /// 别的错误原样透传——把无关的报错也套上主机密钥的提示只会误导。
    #[test]
    fn unrelated_errors_pass_through_untouched() {
        let original = "Permission denied (publickey).";
        assert_eq!(augment_host_key_error(original, &host()), original);
    }

    /// 目的地以 `-` 开头会被 OpenSSH 当成选项（`-oProxyCommand=…` 即任意执行），
    /// 所以校验必须在拼 argv 之前，而不是靠 `--` ——`--` 只保护它后面的参数。
    #[test]
    fn a_leading_dash_host_is_refused() {
        let mut evil = host();
        evil.host = "-oProxyCommand=calc.exe".into();
        assert!(ssh_destination(&evil).is_err());
        assert!(ssh_args(&evil).is_err());
    }
}

/// `test_remote_model_host` 的 SSRF 笼。
///
/// 这条命令接受任意 URL 并把响应回渲染进程。它的**用途**是探测局域网里的
/// Ollama / vLLM，所以不能照搬「只放行公网」；但回环、链路本地、未指定地址
/// 必须挡住——本机 1421 网关对回环免鉴权，`169.254.169.254` 是云元数据端点。
#[cfg(test)]
mod lan_guard_tests {
    use super::*;

    #[test]
    fn loopback_is_refused() {
        for url in [
            "http://127.0.0.1:11434/v1",
            "http://127.5.5.5:8080",
            "http://localhost:11434/v1",
            "http://[::1]:1421",
            "http://[::ffff:127.0.0.1]:1421",
            "http://0.0.0.0:1421",
        ] {
            assert!(
                guard_lan_url(url).is_err(),
                "{url} 应当被拒——本机网关对回环免鉴权"
            );
        }
    }

    #[test]
    fn link_local_and_cloud_metadata_are_refused() {
        for url in [
            "http://169.254.169.254/latest/meta-data/",
            "http://169.254.1.1:8080",
            "http://[fe80::1]:8080",
        ] {
            assert!(guard_lan_url(url).is_err(), "{url} 应当被拒");
        }
    }

    /// **这一条是反向的**：局域网地址必须**放行**，否则功能本身就没了。
    /// 笼子收得过紧和不收一样是 bug。
    #[test]
    fn private_lan_addresses_are_allowed_because_that_is_the_point() {
        for url in [
            "http://192.168.1.10:11434/v1",
            "http://10.0.0.5:8000",
            "http://172.16.3.4:11434",
        ] {
            assert!(
                guard_lan_url(url).is_ok(),
                "{url} 被误拒了——探测局域网模型主机正是这个功能的用途"
            );
        }
    }

    #[test]
    fn non_http_schemes_are_refused() {
        assert!(guard_lan_url("file:///etc/passwd").is_err());
        assert!(guard_lan_url("ftp://192.168.1.10/").is_err());
    }

    /// **命令必须真的调这个笼子。**
    ///
    /// 上面几条只测纯函数——把 `guard_lan_url(&base)?` 从命令里删掉，它们照样全绿。
    /// 反向验证当场暴露了这一点：笼子造好了、没接上，和没造一样。
    /// 这条直接调命令本身（它不吃 State，能在测试里调）。
    #[tokio::test]
    async fn the_command_itself_refuses_loopback() {
        let err = test_remote_model_host("http://127.0.0.1:1421/v1".into())
            .await
            .expect_err("命令没有调用 guard_lan_url——回环被放行了");
        assert!(err.contains("本机"), "错误信息没说清原因：{err}");
    }
}
