use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use crate::proc::NoWindow;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::db::DbManager;
use crate::runtime::{managed_install_command, AgentId};


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

/// 唯一一份 CLI agent 清单：显示名 → 可执行文件名。
///
/// 曾经存过三份。「设置 → 诊断」那份查的是 `gemini-cli`，而真实二进制叫
/// `gemini`，于是同一台机器上诊断页说「未检测到安装」、智能体页说「已安装」。
/// 检测、诊断、路径解析现在都读这一个常量。
pub(crate) const CLI_AGENTS: &[(&str, &str)] = &[
    ("Claude Code", "claude"),
    ("Gemini CLI", "gemini"),
    ("Codex", "codex"),
    ("Qwen Code", "qwen"),
    ("GitHub Copilot CLI", "copilot"),
    ("Google Antigravity", "agy"),
    ("OpenCode", "opencode"),
    ("Grok Build", "grok"),
];

pub(crate) fn agent_slug(agent_name: &str) -> &'static str {
    match agent_name {
        "Claude Code" => "claude-code",
        "Codex" => "codex",
        "Gemini CLI" => "gemini-cli",
        "OpenCode" => "opencode",
        "Qwen Code" => "qwen-code",
        "GitHub Copilot CLI" => "github-copilot-cli",
        "Google Antigravity" => "antigravity",
        "Grok Build" => "grok-build",
        _ => "custom-agent",
    }
}

pub(crate) fn managed_agent_root(db: &DbManager, agent_name: &str) -> PathBuf {
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

pub(crate) fn executable_in_managed_root(root: &Path, bin_name: &str) -> Option<String> {
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

/// Update status of an installed agent CLI: its installed version vs the latest
/// published on npm.
#[derive(Debug, Clone, Serialize)]
pub struct AgentUpdateInfo {
    pub name: String,
    pub current: String,
    pub latest: Option<String>,
    pub has_update: bool,
    /// npm package name, or `None` for agents not distributed via npm.
    pub package: Option<String>,
}

/// Install spec for Grok Build.
///
/// Deliberately **not** `@latest`: xAI's `latest` dist-tag points at 0.1.4, whose
/// manifest declares `os: ["darwin"], cpu: ["arm64"]`, so `npm install` aborts with
/// EBADPLATFORM (exit 1) on Windows and Linux. The published 0.2.x line declares all
/// six platform binaries (darwin/linux/win32 × arm64/x64), so pin to that line and
/// let npm resolve against real versions rather than the stale tag.
///
/// The `0.2` form is load-bearing, not shorthand for `^0.2.0`: on Windows we invoke
/// `npm.cmd`, a batch file, so the argument is parsed by cmd.exe — where `^` is the
/// escape character. A `^0.2.0` spec arrives as the exact version `0.2.0` and silently
/// pins users to the oldest release in the line. `0.2` is the same `>=0.2.0 <0.3.0`
/// range with no shell metacharacters, and resolves identically under sh and cmd.exe.
pub const GROK_NPM_SPEC: &str = "@xai-official/grok@0.2";

/// OMNIX 在别的 AI 应用的 `mcpServers` 里用的键名。稳定不变——改了会在用户配置里
/// 留下一个孤儿条目，而我们再也认不出它是自己写的。
pub const OMNIX_MCP_KEY: &str = "omnix";

/// OMNIX 在别人配置里长什么样：一个指向本机网关 `/mcp` 的远程 MCP 服务器。
///
/// 走 loopback，所以不用带令牌——网关对本机请求本来就放行（见
/// `proxy::guard_gateway_access`）。把令牌写进别人的配置文件反而是在到处撒密钥。
fn omnix_mcp_entry(port: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "http",
        "url": format!("http://127.0.0.1:{port}/mcp"),
    })
}

/// Claude Desktop 的配置目录（各平台不同）。返回 `None` = 这个平台我们没适配，
/// 那就什么都别做，而不是往一个猜出来的路径写文件。
fn claude_desktop_config_dir(home: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        Some(home.join("AppData/Roaming/Claude"))
    }
    #[cfg(target_os = "macos")]
    {
        Some(home.join("Library/Application Support/Claude"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Some(home.join(".config/Claude"))
    }
}

/// 写文件：先写临时文件再原子改名。临时名带进程号，两个 OMNIX 同时跑也不会
/// 互相写同一个临时文件、把半截内容改名成正式配置。
fn atomic_write(file_path: &Path, content: &str) -> Result<(), String> {
    let tmp_path = file_path.with_extension(format!("omnix-{}.tmp", std::process::id()));
    fs::write(&tmp_path, content).map_err(|e| format!("写临时文件失败: {e}"))?;
    fs::rename(&tmp_path, file_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("原子替换配置文件失败: {e}")
    })
}

/// 安全地改写**别人家**的 JSON 配置文件，返回是否真的写了。
///
/// 这里的每一条规矩都是为了同一件事：**宁可什么都不做，也不能把用户配好的东西
/// 弄丢。** 原来的写法是「读失败或解析失败就当成空对象，然后写回去」——用户的
/// Claude Desktop 配置只要有一个逗号写错，所有 MCP 服务器就被清空了。
///
/// - 文件读不出来（权限、被占用）→ 放弃，不写
/// - 有内容但不是合法 JSON → 放弃，不写（**绝不**回落成 `{}`）
/// - 顶层不是对象 → 放弃，不写
/// - 改完跟改之前一模一样 → 不写（少写一次就少一次损坏机会）
fn merge_json_config(
    path: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> Result<bool, String> {
    let mut val = if path.exists() {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("读不了 {}，已跳过（未改动）：{e}", path.display()))?;
        if content.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str::<serde_json::Value>(&content)
                .map_err(|e| format!("{} 不是合法 JSON，已跳过（未改动）：{e}", path.display()))?
        }
    } else {
        serde_json::json!({})
    };

    let Some(obj) = val.as_object_mut() else {
        return Err(format!("{} 顶层不是 JSON 对象，已跳过（未改动）", path.display()));
    };
    let before = obj.clone();
    edit(obj);
    if *obj == before {
        return Ok(false);
    }
    atomic_write(path, &val.to_string())?;
    Ok(true)
}

/// The npm package an agent CLI ships as (without the `@latest` tag), or `None`
/// for agents installed by a non-npm mechanism (e.g. Antigravity's installer).
pub fn npm_package_for_agent(display_name: &str) -> Option<&'static str> {
    match display_name {
        "Claude Code" => Some("@anthropic-ai/claude-code"),
        "Codex" => Some("@openai/codex"),
        // Version spec lives in GROK_NPM_SPEC; this map is package-name only.
        "Gemini CLI" => Some("@google/gemini-cli"),
        "Qwen Code" => Some("@qwen-code/qwen-code"),
        "OpenCode" => Some("opencode-ai"),
        "Grok Build" => Some("@xai-official/grok"),
        "GitHub Copilot CLI" => Some("@github/copilot-cli"),
        _ => None,
    }
}

/// Extracts the first `x.y.z` semver from a CLI `--version` string, which often
/// carries extra text (e.g. "codex-cli 0.9.1 (rust)").
pub fn extract_semver(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut dots = 0;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    dots += 1;
                }
                i += 1;
            }
            if dots >= 2 {
                return Some(raw[start..i].trim_end_matches('.').to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// True when `current` is a strictly older semver than `latest`. Missing/unparsable
/// components compare as 0; a parse failure returns false (never nags spuriously).
pub fn semver_is_older(current: &str, latest: &str) -> bool {
    fn parts(v: &str) -> [u64; 3] {
        let mut out = [0u64; 3];
        for (i, seg) in v.split('.').take(3).enumerate() {
            out[i] = seg
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
        }
        out
    }
    parts(current) < parts(latest)
}

pub struct AgentManager {
    db: Arc<DbManager>,
}

/// Where the injected lessons are written, relative to the workspace.
/// Kept out of the agent context files on purpose — see the write site.
pub(crate) const MEMORY_SIDECAR_DIR: &str = ".omnix";
pub(crate) const MEMORY_SIDECAR_FILE: &str = "memory.md";

/// The managed block that goes into CLAUDE.md / AGENTS.md / GEMINI.md.
/// Deliberately contains no lesson text — only a path — so committing the
/// context file never publishes the user's memory bank.
fn memory_pointer_block(sidecar_rel: &str) -> String {
    format!(
        "\n<!--- OMNIX MEMORY START --->\n\
         ## 🧠 OMNIX Anti-Failure Guidelines\n\
         改动本项目前，先读 `{sidecar_rel}`：那里是与本工作区相关的历史踩坑记录与规约，\
         请严加防范、避免重犯。（该文件是本机个人经验，已被 .gitignore 排除，不随仓库分发。）\n\
         <!--- OMNIX MEMORY END --->\n"
    )
}

/// Make sure the sidecar can't be committed. Appends to `.gitignore` only
/// inside a git repo, only once, and never rewrites existing entries.
fn ensure_sidecar_ignored(workspace_path: &Path) {
    if !workspace_path.join(".git").exists() {
        return; // not a repo — nothing to leak into
    }
    let gitignore = workspace_path.join(".gitignore");
    let existing = fs::read_to_string(&gitignore).unwrap_or_default();
    let entry = format!("{MEMORY_SIDECAR_DIR}/");
    if existing.lines().any(|l| {
        let l = l.trim();
        l == entry || l == MEMORY_SIDECAR_DIR || l == format!("/{entry}")
    }) {
        return;
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(
        "\n# OMNIX 注入的个人踩坑经验（本机资产，不随仓库分发）\n",
    );
    next.push_str(&entry);
    next.push('\n');
    let _ = fs::write(&gitignore, next);
}

impl AgentManager {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self {
            db,
        }
    }

    pub fn start_services(&self) {
        // 这里以前还有一个 `start_idle_reaper()`——它回收的是 PTY 子进程，
        // 随「兼容终端」那条链一起删了。runtime 会话有自己的生命周期管理。
        self.start_cron_scheduler();
        self.start_autopilot_scheduler();
    }

    /// Polls active autopilots and, when one is due, enqueues
    /// a reviewable run (creates a conversation + a `queued` autopilot_run). The
    /// frontend claims queued runs and executes them through the real runtime.
    /// DB-only, mirroring `start_cron_scheduler`; reuses `match_schedule`.
    fn start_autopilot_scheduler(&self) {
        let db = Arc::clone(&self.db);
        tauri::async_runtime::handle().spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                // Scope the connection/statement so they drop before we fire (which
                // opens its own connection); avoids holding the read across writes.
                let due: Vec<String> = {
                    let Ok(conn) = db.get_connection() else { continue };
                    let mut stmt = match conn.prepare(
                        "SELECT id, schedule, last_run FROM autopilots WHERE enabled = 1",
                    ) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let rows = stmt.query_map([], |row| {
                        let last_run_str: Option<String> = row.get(2)?;
                        let last_run = last_run_str.and_then(|s| {
                            chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                .ok()
                                .and_then(|ndt| chrono::Local.from_local_datetime(&ndt).single())
                        });
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, last_run))
                    });
                    match rows {
                        Ok(rows) => rows
                            .flatten()
                            .filter(|(_, schedule, last_run)| match_schedule(schedule, *last_run))
                            .map(|(id, _, _)| id)
                            .collect(),
                        Err(_) => continue,
                    }
                };
                for id in due {
                    if let Err(e) = crate::commands::fire_autopilot_run(&db, &id, "schedule") {
                        log::warn!("autopilot scheduler: failed to fire {id}: {e}");
                    } else {
                        log::info!("autopilot scheduler: enqueued run for {id}");
                    }
                }
            }
        });
    }

    // --- 1. Agent Detection logic ---
    pub fn detect_agents(&self) -> Vec<DetectedAgent> {
        let mut list = Vec::new();

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

        for (display_name, _) in CLI_AGENTS {
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
            .no_window()
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
            // 写失败要说出来，而且**不能照样报成功**。
            //
            // 这里以前是 `let _ = fs::write(...)` 后面无条件打印「已预置」——写不进去
            // 时日志里仍然是一句成功。而真失败的后果很具体：Claude Code 启动后会弹
            // 交互式 ToS 提示，我们是当子进程拉起来的，没人能回答那个提示，agent 就
            // 那么挂着。用户看到的是「点了启动没反应」，日志却说预置成功。
            match fs::write(&config_file, tos_bypass_json.to_string()) {
                Ok(()) => println!(
                    "Pre-seeded Claude Code configuration to bypass initial TOS interactive prompt."
                ),
                Err(error) => log::warn!(
                    "预置 Claude Code 配置失败（{}）：{error}。\
                     它启动时可能停在交互式 ToS 提示上而无法继续。",
                    config_file.display()
                ),
            }
        }
    }
}

impl AgentManager {
    pub async fn install_agent(&self, agent_name: &str) -> Result<(), String> {
        if agent_name == "Qwen Code" {
            return Err("Qwen Code managed installation is not supported yet; OMNIX will not create a mock CLI".into());
        }

        if agent_name == "Google Antigravity" {
            let mut cmd = if cfg!(windows) {
                let mut c = Command::new("powershell");
                c.args([
                    "-Command",
                    "irm https://antigravity.google/cli/install.ps1 | iex",
                ]);
                c
            } else {
                let mut c = Command::new("sh");
                c.args([
                    "-c",
                    "curl -fsSL https://antigravity.google/cli/install.sh | bash",
                ]);
                c
            };
            cmd.no_window()
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let child = cmd
                .spawn()
                .map_err(|e| format!("Failed to spawn Antigravity installer: {}", e))?;
                // wait_with_output 会持续排空管道。只 wait() 不读时，安装脚本输出
                // 一旦超过管道缓冲区（Windows 4-64KB），子进程阻塞在写端，父进程
                // 等它退出 -> 双向死锁，UI 永久卡死（历史事故）。
            let output = child
                .wait_with_output()
                .await
                .map_err(|e| format!("Antigravity installer run error: {}", e))?;
            if output.status.success() {
                return Ok(());
            } else {
                let tail = |bytes: &[u8]| {
                    String::from_utf8_lossy(bytes)
                        .lines()
                        .rev()
                        .take(10)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                return Err(format!(
                    "Antigravity installer failed with code {:?}: {}",
                    output.status.code(),
                    tail(&output.stderr)
                ));
            }
        }

        let core_agent = match agent_name {
            "Claude Code" => Some(AgentId::ClaudeCode),
            "Codex" => Some(AgentId::Codex),
            "Gemini CLI" => Some(AgentId::GeminiCli),
            "Qwen Code" => Some(AgentId::QwenCode),
            "OpenCode" => Some(AgentId::OpenCode),
            "GitHub Copilot CLI" => Some(AgentId::CopilotCli),
            _ => None,
        };
        let package = match agent_name {
            "Gemini CLI" => "@google/gemini-cli@latest",
            "GitHub Copilot CLI" => "@github/copilot-cli@latest",
            "OpenCode" => "opencode-ai@latest",
            // NOT `@latest`: xAI's `latest` dist-tag still points at 0.1.4, which
            // declares `os: darwin, cpu: arm64` and so fails EBADPLATFORM on
            // Windows/Linux. The 0.2.x line ships all six platform binaries.
            "Grok Build" => GROK_NPM_SPEC,
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
            .no_window()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run npm command: {}", e))?;
            // 同 Antigravity 安装器：wait_with_output 排空管道，防止 npm 输出
            // 超过管道缓冲区时双向死锁。
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("Npm install process error: {}", e))?;
        let status = output.status;

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

    /// 把 OMNIX 的设置同步进**其它 AI 应用**的配置文件。
    ///
    /// 这些是别人家的文件，出错代价是删掉用户自己配的东西，所以规矩比改我们自己
    /// 的配置严得多——具体见 [`merge_json_config`]。一个文件坏了不影响其它文件，
    /// 但会在返回值里说清楚哪个跳过了、为什么。
    pub fn sync_agent_configs(&self) -> Result<(), String> {
        let home_dir = dirs::home_dir().ok_or("找不到用户主目录，已跳过配置同步")?;
        let mut skipped: Vec<String> = Vec::new();

        // A. Claude Code
        let claude_code_config = home_dir.join(".config/claude-code/config.json");
        if claude_code_config
            .parent()
            .map(|p| p.exists())
            .unwrap_or(false)
        {
            if let Err(e) = merge_json_config(&claude_code_config, |obj| {
                obj.insert("tosAccepted".into(), serde_json::Value::Bool(true));
                obj.insert("analyticsConsent".into(), "opt-out".into());
            }) {
                skipped.push(e);
            }
        }

        // B. Claude Desktop —— 把 OMNIX 注册成一个 MCP 服务器，让它的能力
        // 在 Claude Desktop 里也用得上（P1 的对外入口）。
        let claude_desktop_dir = claude_desktop_config_dir(&home_dir);
        if claude_desktop_dir.as_ref().is_some_and(|d| d.exists()) {
            let port = self
                .db
                .get_setting("proxy_port")
                .unwrap_or(None)
                .unwrap_or_else(|| "1421".to_string());
            let path = claude_desktop_dir.unwrap().join("claude_desktop_config.json");
            match merge_json_config(&path, |obj| {
                // 只加/更新我们那一项，别人配的 MCP 服务器原样保留。
                let servers = obj
                    .entry("mcpServers")
                    .or_insert_with(|| serde_json::json!({}));
                if !servers.is_object() {
                    *servers = serde_json::json!({});
                }
                if let Some(m) = servers.as_object_mut() {
                    m.insert(OMNIX_MCP_KEY.into(), omnix_mcp_entry(&port));
                }
            }) {
                Ok(true) => println!("已把 OMNIX 注册进 Claude Desktop 的 MCP 配置"),
                Ok(false) => {}
                Err(e) => skipped.push(e),
            }
        }

        if skipped.is_empty() {
            Ok(())
        } else {
            Err(skipped.join("；"))
        }
    }

    pub async fn uninstall_agent(&self, agent_name: &str) -> Result<(), String> {
        let sandbox_dir = managed_agent_root(&self.db, agent_name);

        if agent_name == "Codex" || agent_name == "Qwen Code" {
            let bin_name = if agent_name == "Codex" {
                "codex"
            } else {
                "qwen"
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
                "Grok Build" => "@xai-official/grok",
                _ => return Err(format!("Unsupported agent CLI uninstall: {}", agent_name)),
            };

            let bin_name = match agent_name {
                "Claude Code" => "claude",
                "Gemini CLI" => "gemini",
                "GitHub Copilot CLI" => "copilot",
                "OpenCode" => "opencode",
                "Grok Build" => "grok",
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


}

/// 把记忆库回注进工作区的 CLAUDE.md / AGENTS.md / GEMINI.md（外加 `.omnix/memory.md`
/// sidecar）。
///
/// 以前它是 `spawn_agent` 里的一行——也就是说只有**启动 PTY 会话**时才会回注。
/// PTY 那条路早已不可达（`start_agent_session` 没有任何调用方），所以这个功能跟着
/// 静默停摆了：`build_memory_block` 一直好好的、`evolution.rs` 的文档也一直说
/// 「回注由 inject_workspace_memories 负责」，但没有任何东西再调用它。
///
/// 现在挂在 `RuntimeManager::start_session` 上——runtime 会话就是当年 spawn_agent
/// 的对应位置。它只用到 `db`，本来就不需要 AgentManager。
pub(crate) fn inject_workspace_memories(
    db: &DbManager,
    workspace_dir: &str,
    agent_name: &str,
) -> Result<(), String> {
    // Build the managed memory block (relevance-ranked; shared with the
    // evolution preview command). None when there are no experience memories.
    let memories_md = match crate::commands::build_memory_block(db, workspace_dir)? {
        Some(block) => block,
        None => return Ok(()),
    };

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

    // The lessons themselves live in a gitignored sidecar, never in the
    // agent context file. Those files (CLAUDE.md / AGENTS.md / GEMINI.md) are
    // normally committed and shared, while the memory bank records what THIS
    // user hit on THIS machine — personal data that must not ride along with
    // a repo. The context file only gets a neutral pointer, which is safe to
    // commit and still routes every agent to the lessons.
    let sidecar_rel = format!("{MEMORY_SIDECAR_DIR}/{MEMORY_SIDECAR_FILE}");
    let sidecar_dir = workspace_path.join(MEMORY_SIDECAR_DIR);
    // 边车文件写不成 = **记忆整个功能静默不生效**：agent 照常启动、照常干活，
    // 只是永远看不到那些教训，而界面上没有任何迹象。这类「功能还在、只是不起作用」
    // 的失败最难发现，所以至少要在日志里留一行。
    //
    // 仍然不中断流程：拿不到记忆比起不动 agent 要好得多。
    if let Err(error) = fs::create_dir_all(&sidecar_dir) {
        log::warn!("建记忆边车目录失败（{}）：{error}——本次不会注入记忆", sidecar_dir.display());
    } else if let Err(error) = fs::write(sidecar_dir.join(MEMORY_SIDECAR_FILE), &memories_md) {
        log::warn!("写记忆边车文件失败（{}）：{error}——本次不会注入记忆", sidecar_dir.display());
    }
    ensure_sidecar_ignored(&workspace_path);

    let pointer = memory_pointer_block(&sidecar_rel);
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
                    // Replaces any previously inlined lessons too, so upgrading
                    // scrubs personal content out of an already-written file.
                    content.replace_range(start_idx..actual_end, &pointer);
                } else {
                    content.push_str(&pointer);
                }
                // 指针写不进去，agent 就不知道去哪找记忆——边车文件写成了也白写。
                // 和上面同一个道理：只记日志，不中断启动。
                if let Err(error) = fs::write(&file_path, content) {
                    log::warn!("更新记忆指针失败（{}）：{error}", file_path.display());
                }
            }
        } else if let Err(error) = fs::write(&file_path, &pointer) {
            log::warn!("写入记忆指针失败（{}）：{error}", file_path.display());
        }
    }

Ok(())
}

impl AgentManager {
    pub fn find_agent_path(&self, display_name: &str) -> Option<String> {
        Self::find_agent_path_static(display_name, Some(&self.db))
    }

    pub fn find_agent_path_static(display_name: &str, db: Option<&DbManager>) -> Option<String> {
        let bin_name = CLI_AGENTS.iter().find(|(dn, _)| *dn == display_name)?.1;

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

/// 一条**认得出来**的定时表达式。
///
/// 抽出来是因为它有两个用途：调度器判断「现在该不该跑」，保存时判断「这串东西
/// 到底认不认得」。以前只有前者，语法藏在 `match_schedule` 里；于是一条拼错的
/// 表达式能存进库、界面上显示为「已启用」，然后**永远不触发，也没有任何报错**。
/// 两边共用这一份解析，语法就不会漂开。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Schedule {
    EveryMinutes(i64),
    EveryHours(i64),
    /// 每天的固定时刻（时, 分）。
    DailyAt(u32, u32),
    /// 五字段 cron：分 时 日 月 周。
    Cron(Vec<CronField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CronField {
    Any,
    Step(u32),
    Exact(u32),
}

impl CronField {
    fn parse(field: &str) -> Option<Self> {
        if field == "*" {
            return Some(CronField::Any);
        }
        if let Some(step) = field.strip_prefix("*/") {
            // `*/0` 会在取模时除零，不认。
            return step
                .parse::<u32>()
                .ok()
                .filter(|s| *s > 0)
                .map(CronField::Step);
        }
        field.parse::<u32>().ok().map(CronField::Exact)
    }

    fn matches(&self, current: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Step(step) => current.is_multiple_of(*step),
            CronField::Exact(value) => *value == current,
        }
    }
}

/// 人给的这串东西认不认得。认不出来返回 `None`——调度器当作「不该跑」，保存
/// 那一步当作「拒绝」。
///
/// **不支持** cron 的区间和列表（`1-5`、`1,3,5`）：`CronField` 只认 `*`、`*/N`、
/// 纯数字。以前这类表达式会被存下来然后静默不跑，现在保存就会拒。
pub(crate) fn parse_schedule(schedule: &str) -> Option<Schedule> {
    let schedule = schedule.trim().to_lowercase();

    if let Some(rest) = schedule
        .strip_prefix("every ")
        .and_then(|s| s.strip_suffix(" minutes"))
    {
        return rest.trim().parse::<i64>().ok().map(Schedule::EveryMinutes);
    }
    if let Some(rest) = schedule
        .strip_prefix("every ")
        .and_then(|s| s.strip_suffix(" hours"))
    {
        return rest.trim().parse::<i64>().ok().map(Schedule::EveryHours);
    }
    if let Some(time_str) = schedule.strip_prefix("daily at ") {
        let parts: Vec<&str> = time_str.trim().split(':').collect();
        if parts.len() == 2 {
            if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if h < 24 && m < 60 {
                    return Some(Schedule::DailyAt(h, m));
                }
            }
        }
        // `daily at` 开头但时刻不合法，不再往下当 cron 解析。
        return None;
    }

    let fields: Vec<&str> = schedule.split_whitespace().collect();
    if fields.len() == 5 {
        let parsed: Option<Vec<CronField>> = fields.iter().map(|f| CronField::parse(f)).collect();
        return parsed.map(Schedule::Cron);
    }
    None
}

/// 人能读的支持形式说明，保存被拒时原样回给界面。
pub(crate) const SCHEDULE_HELP: &str = "支持 `every N minutes`、`every N hours`、`daily at HH:MM`，或五字段 cron（分 时 日 月 周，每段只能是 *、*/N 或数字；不支持 1-5、1,3,5 这类区间和列表）";

impl Schedule {
    fn is_due(&self, now: DateTime<Local>, last_run: Option<DateTime<Local>>) -> bool {
        match self {
            Schedule::EveryMinutes(minutes) => match last_run {
                Some(lr) => (now - lr).num_minutes() >= *minutes,
                None => true,
            },
            Schedule::EveryHours(hours) => match last_run {
                Some(lr) => (now - lr).num_hours() >= *hours,
                None => true,
            },
            Schedule::DailyAt(hour, minute) => {
                if now.hour() != *hour || now.minute() != *minute {
                    return false;
                }
                match last_run {
                    Some(lr) => lr.date_naive() != now.date_naive(),
                    None => true,
                }
            }
            Schedule::Cron(fields) => {
                let current = [
                    now.minute(),
                    now.hour(),
                    now.day(),
                    now.month(),
                    now.weekday().num_days_from_sunday(), // 0 (Sun) - 6 (Sat)
                ];
                fields
                    .iter()
                    .zip(current)
                    .all(|(field, value)| field.matches(value))
            }
        }
    }
}

fn match_schedule(schedule: &str, last_run: Option<DateTime<Local>>) -> bool {
    let now = Local::now();

    if let Some(lr) = last_run {
        if (now - lr).num_seconds() < 55 {
            return false; // Prevent double trigger in the same minute
        }
    }

    parse_schedule(schedule).is_some_and(|parsed| parsed.is_due(now, last_run))
}

pub(crate) async fn run_cron_task(
    db: Arc<DbManager>,
    task_id: String,
    agent_name: String,
    args_str: String,
    workspace_dir: String,
) -> Result<(), String> {
    // 这两件事必须在**最前面**：上一轮还没结束时，连查 agent 装没装都不该做。
    //
    // 同一个任务不叠加——定时周期短于单次耗时是很常见的配置错误，不拦的话会越堆
    // 越多，最后把机器占满。
    let timeout_min = (cron_timeout(&db).as_secs() / 60).max(1);
    if let Ok(conn) = db.get_connection() {
        // 先把陈旧的 'running' 收干净：应用被强杀、或这次修复之前卡住的那些，
        // 会永远停在 running。不清理的话下面这道保护会把任务永久锁死——
        // 一道保护措施把功能彻底关掉，比没有保护更糟。
        let _ = conn.execute(
            "UPDATE cron_runs SET status = 'timeout', finished_at = CURRENT_TIMESTAMP
             WHERE status = 'running'
               AND started_at < datetime('now', ?1)",
            params![format!("-{} minutes", timeout_min)],
        );
        let running: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cron_runs WHERE task_id = ?1 AND status = 'running'",
                params![task_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if running > 0 {
            let msg = "上一轮还在运行，本轮跳过".to_string();
            log_cron_run_skipped(&db, &task_id, &msg).await;
            return Err(msg);
        }
    }

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

    // 这里以前有一条 WSL 分支：`use_wsl` 为真就改用 `wsl.exe -d <发行版>` 起 agent，
    // 并把 ANTHROPIC_BASE_URL 指向宿主机路由 IP。整段删除，理由见
    // `proxy_auth::decide_gateway_access` 上的说明——那个开关根本不落盘，
    // 这条分支从没跑过；而它要成立就得让网关对局域网免令牌敞开。
    let proxy_port = db
        .get_setting("proxy_port")
        .unwrap_or(None)
        .unwrap_or_else(|| "1421".to_string());
    let local_proxy_url = format!(
        "http://localhost:{}/agent/{}",
        proxy_port,
        agent_name.replace(' ', "_")
    );
    let mut cmd = {
        let mut c = Command::new(&exe_path);
        c.args(args)
            .env("ANTHROPIC_BASE_URL", &local_proxy_url)
            .env("CLAUDE_CODE_HEADLESS", "1")
            .env("DISABLE_UPDATES", "1")
            .env("DISABLE_AUTOUPDATER", "1");
        c
    };

    cmd.current_dir(resolved_workspace)
        .no_window()
        // stdin 必须显式置空。不设的话 tokio 默认继承，子进程碰到交互提示时
        // 的行为就取决于应用是怎么被启动的（有没有控制台），而不是确定的。
        // 置空 = 任何等输入的地方立刻拿到 EOF，快速失败而不是挂住。
        .stdin(Stdio::null())
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

    // 定时任务必须有上限。没有超时的后台进程是慢性泄漏：卡住的那次永远停在
    // 'running'，进程不退，而下一次触发照样再开一个——跑一夜能攒出一堆。
    // 卡住的原因可以是网络挂起、agent 等一个永远不会来的输入、或者它自己死循环，
    // 这里不区分，一律按超时处理。
    let limit = cron_timeout(&db);
    // 时间窗的起点。网关只知道是哪个 agent 在请求，不知道这是哪一次定时运行，
    // 所以用时间窗关联——这是已知的近似，摘要里说的是「这段时间内」。
    let started_at = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let timed_out = match tokio::time::timeout(limit, child.wait()).await {
        Ok(status) => {
            let _ = log_writer.await;
            let success = matches!(status, Ok(s) if s.success());
            log_cron_run_status(&db, &run_id, if success { "success" } else { "failed" }).await;
            summarize_run(&db, &run_id, &agent_name, &started_at).await;
            false
        }
        Err(_) => {
            // 先杀进程再收日志：不杀的话 log_writer 会一直等一个不会关闭的管道。
            let _ = child.kill().await;
            let _ = log_writer.await;
            log_cron_run_status(&db, &run_id, "timeout").await;
            summarize_run(&db, &run_id, &agent_name, &started_at).await;
            true
        }
    };
    if timed_out {
        return Err(format!(
            "定时任务超时（{} 分钟未结束），已终止",
            limit.as_secs() / 60
        ));
    }

    Ok(())
}

/// 定时任务的单次上限。可用 `cron_timeout_minutes` 设置调整；
/// 默认 30 分钟——足够跑完一次正经的 agent 任务，又不至于卡一整夜。
/// 设成 0 表示不限制（明确选择放弃这道保护，不是默认行为）。
fn cron_timeout(db: &DbManager) -> Duration {
    let minutes = db
        .get_setting("cron_timeout_minutes")
        .unwrap_or(None)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    if minutes == 0 {
        Duration::from_secs(u64::MAX / 2) // 实质不限制，又不会溢出
    } else {
        Duration::from_secs(minutes * 60)
    }
}

async fn log_cron_run_status(db: &DbManager, run_id: &str, status: &str) {
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE cron_runs SET status = ?1, finished_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status, run_id],
        );
    }
}

/// 记一条「跳过」。用独立状态而不是 failed——跳过是保护动作，
/// 混进失败里会让人以为任务坏了，反而去关掉这道保护。
async fn log_cron_run_skipped(db: &DbManager, task_id: &str, reason: &str) {
    let run_id = format!("run_skip_{}_{}", task_id, Local::now().format("%Y%m%d_%H%M%S"));
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "INSERT INTO cron_runs (id, task_id, status, log_path, started_at, finished_at)
             VALUES (?1, ?2, 'skipped', ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![run_id, task_id, reason],
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

    #[test]
    fn extract_semver_pulls_version_from_noisy_output() {
        assert_eq!(extract_semver("0.9.1").as_deref(), Some("0.9.1"));
        assert_eq!(
            extract_semver("codex-cli 0.9.1 (rust)").as_deref(),
            Some("0.9.1")
        );
        assert_eq!(
            extract_semver("gemini-cli version 0.46.0").as_deref(),
            Some("0.46.0")
        );
        assert_eq!(extract_semver("no version here").as_deref(), None);
        // Two-component strings are not a semver we act on.
        assert_eq!(extract_semver("v1.2").as_deref(), None);
    }

    #[test]
    fn semver_older_detects_available_update() {
        assert!(semver_is_older("0.46.0", "0.47.0"));
        assert!(semver_is_older("1.2.3", "1.2.4"));
        assert!(semver_is_older("1.9.0", "2.0.0"));
        assert!(!semver_is_older("0.47.0", "0.47.0"));
        assert!(!semver_is_older("1.0.0", "0.9.9"));
    }

    #[test]
    fn npm_package_map_covers_acp_agents() {
        assert_eq!(npm_package_for_agent("OpenCode"), Some("opencode-ai"));
        assert_eq!(
            npm_package_for_agent("Qwen Code"),
            Some("@qwen-code/qwen-code")
        );
        assert_eq!(npm_package_for_agent("Google Antigravity"), None);
    }

    /// Guards the per-agent context-file injection. Runs in CI: it only touches
    /// a temp DB + temp workspace. (It was `#[ignore]`d and silently rotted —
    /// it still asserted an `OMNIX_MEMORY.md` that injection stopped writing
    /// once context files became per-agent.)
    #[tokio::test]
    async fn test_memory_injection() {
        let temp_dir = std::env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Unique per run: a fixed name would collide with parallel test threads.
        let test_db_path = temp_dir.join(format!("omnix_agent_test_{}.db", timestamp));

        let db = Arc::new(DbManager::new_with_path(test_db_path.clone()));

        let manager = AgentManager::new(Arc::clone(&db));

        let test_workspace = temp_dir.join(format!("omnix_workspace_{}", timestamp));
        fs::create_dir_all(&test_workspace).unwrap();

        // Run injection
        inject_workspace_memories(&manager.db, &test_workspace.to_string_lossy(), "Claude Code")
            .unwrap();

        // "Claude Code" gets CLAUDE.md only — injection writes the context file
        // the requested agent actually reads, not a shared memory dump.
        let claude_md = test_workspace.join("CLAUDE.md");
        assert!(claude_md.exists(), "CLAUDE.md 应被注入");
        assert!(
            !test_workspace.join("GEMINI.md").exists(),
            "只请求 Claude Code 时不应写其他 agent 的上下文文件"
        );

        // The context file is committable: it points at the lessons, it does not
        // contain them. This is the privacy guarantee — CLAUDE.md/AGENTS.md are
        // normally shared, the memory bank is personal to this machine.
        let content = fs::read_to_string(&claude_md).unwrap();
        assert!(content.contains("OMNIX Anti-Failure Guidelines"));
        assert!(
            content.contains(".omnix/memory.md"),
            "上下文文件应指向旁挂文件:\n{content}"
        );
        assert!(
            !content.contains("std::sync::MutexGuard across await point"),
            "经验正文绝不能写进会被提交的上下文文件:\n{content}"
        );

        // The lessons themselves live in the sidecar.
        let sidecar = test_workspace.join(".omnix").join("memory.md");
        let sidecar_body = fs::read_to_string(&sidecar).expect("旁挂记忆文件应存在");
        assert!(
            sidecar_body.contains("std::sync::MutexGuard across await point"),
            "旁挂文件才装经验正文:\n{sidecar_body}"
        );

        // Clean up
        let _ = fs::remove_file(claude_md);
        let _ = fs::remove_dir_all(test_workspace.join(".omnix"));
        let _ = fs::remove_dir(test_workspace);
        if test_db_path.exists() {
            let _ = fs::remove_file(&test_db_path);
        }
    }

    /// In a git repo the sidecar must be gitignored on write, and an older
    /// version that had inlined the lessons must get them scrubbed out.
    #[tokio::test]
    async fn injection_gitignores_sidecar_and_scrubs_inlined_lessons() {
        let temp_dir = std::env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_db_path = temp_dir.join(format!("omnix_ignore_test_{}.db", timestamp));
        let db = Arc::new(DbManager::new_with_path(test_db_path.clone()));
        let manager = AgentManager::new(Arc::clone(&db));

        let ws = temp_dir.join(format!("omnix_gitws_{}", timestamp));
        fs::create_dir_all(ws.join(".git")).unwrap();
        // Simulate a workspace upgraded from the old behaviour: lessons inline.
        fs::write(
            ws.join("CLAUDE.md"),
            "# Project rules\nkeep me\n\n<!--- OMNIX MEMORY START --->\n\
             ### ❌ 坑点 1: std::sync::MutexGuard across await point\n\
             <!--- OMNIX MEMORY END --->\n",
        )
        .unwrap();

        inject_workspace_memories(&manager.db, &ws.to_string_lossy(), "Claude Code")
            .unwrap();

        let content = fs::read_to_string(ws.join("CLAUDE.md")).unwrap();
        assert!(content.contains("keep me"), "手写内容不能被破坏:\n{content}");
        assert!(
            !content.contains("MutexGuard across await point"),
            "升级后必须把已内联的经验清出去:\n{content}"
        );

        let gitignore = fs::read_to_string(ws.join(".gitignore")).expect(".gitignore 应被创建");
        assert!(gitignore.contains(".omnix/"), "旁挂目录必须被忽略:\n{gitignore}");

        // Idempotent: a second run must not append a duplicate entry.
        inject_workspace_memories(&manager.db, &ws.to_string_lossy(), "Claude Code")
            .unwrap();
        let again = fs::read_to_string(ws.join(".gitignore")).unwrap();
        assert_eq!(
            again.matches(".omnix/").count(),
            1,
            "重复注入不应重复写 .gitignore:\n{again}"
        );

        let _ = fs::remove_dir_all(&ws);
        if test_db_path.exists() {
            let _ = fs::remove_file(&test_db_path);
        }
    }
}

#[cfg(test)]
mod foreign_config_tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("omnix_cfg_{}_{}", std::process::id(), name));
        let _ = fs::remove_file(&p);
        p
    }

    /// 核心承诺：**宁可什么都不做，也不能把用户配好的东西弄丢。**
    /// 原来的写法是解析失败就当空对象写回去，用户 Claude Desktop 里所有
    /// MCP 服务器会被一次清空。
    #[test]
    fn malformed_json_is_never_overwritten() {
        let p = tmp("malformed.json");
        // 一个多了逗号的配置——手写 JSON 很常见的毛病
        let original = r#"{"mcpServers":{"filesystem":{"command":"npx"},}}"#;
        fs::write(&p, original).unwrap();

        let err = merge_json_config(&p, |o| {
            o.insert("touched".into(), serde_json::Value::Bool(true));
        })
        .unwrap_err();

        assert!(err.contains("不是合法 JSON"), "{err}");
        assert_eq!(fs::read_to_string(&p).unwrap(), original, "文件必须一个字节都没动");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn top_level_non_object_is_never_overwritten() {
        let p = tmp("array.json");
        fs::write(&p, "[1,2,3]").unwrap();
        assert!(merge_json_config(&p, |o| {
            o.insert("x".into(), 1.into());
        })
        .is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), "[1,2,3]");
        let _ = fs::remove_file(&p);
    }

    /// 别人配的 MCP 服务器必须原样保留——我们只加自己那一项。
    #[test]
    fn other_mcp_servers_survive_the_merge() {
        let p = tmp("keep.json");
        fs::write(
            &p,
            r#"{"mcpServers":{"filesystem":{"command":"npx","args":["-y","fs"]}},"theme":"dark"}"#,
        )
        .unwrap();

        assert!(merge_json_config(&p, |o| {
            let s = o.entry("mcpServers").or_insert_with(|| serde_json::json!({}));
            if let Some(m) = s.as_object_mut() {
                m.insert(OMNIX_MCP_KEY.into(), omnix_mcp_entry("1421"));
            }
        })
        .unwrap());

        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["filesystem"]["command"], "npx", "别人的条目要还在");
        assert_eq!(v["theme"], "dark", "无关字段要还在");
        assert_eq!(v["mcpServers"][OMNIX_MCP_KEY]["url"], "http://127.0.0.1:1421/mcp");
        let _ = fs::remove_file(&p);
    }

    /// 没有变化就不写。少写一次就少一次损坏机会，也不会无谓刷新文件时间。
    #[test]
    fn no_change_means_no_write() {
        let p = tmp("noop.json");
        fs::write(&p, r#"{"a":1}"#).unwrap();
        let before = fs::metadata(&p).unwrap().modified().unwrap();
        assert!(!merge_json_config(&p, |_| {}).unwrap(), "没改动不该写");
        assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), before);
        let _ = fs::remove_file(&p);
    }

    /// mcpServers 被写成了别的类型（字符串/数组）时不能 panic，也不能连累别的字段。
    #[test]
    fn wrong_typed_mcp_servers_is_replaced_not_crashed() {
        let p = tmp("wrongtype.json");
        fs::write(&p, r#"{"mcpServers":"oops","keep":true}"#).unwrap();
        assert!(merge_json_config(&p, |o| {
            let s = o.entry("mcpServers").or_insert_with(|| serde_json::json!({}));
            if !s.is_object() {
                *s = serde_json::json!({});
            }
            if let Some(m) = s.as_object_mut() {
                m.insert(OMNIX_MCP_KEY.into(), omnix_mcp_entry("1421"));
            }
        })
        .unwrap());
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert!(v["mcpServers"][OMNIX_MCP_KEY].is_object());
        assert_eq!(v["keep"], true);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn missing_file_is_created_from_scratch() {
        let p = tmp("new.json");
        assert!(merge_json_config(&p, |o| {
            o.insert("tosAccepted".into(), serde_json::Value::Bool(true));
        })
        .unwrap());
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["tosAccepted"], true);
        let _ = fs::remove_file(&p);
    }
}

#[cfg(test)]
mod cron_guard_tests {
    use super::*;

    fn db_at(tag: &str) -> Arc<DbManager> {
        let p = std::env::temp_dir().join(format!("omnix_cron_{}_{tag}.db", std::process::id()));
        let _ = fs::remove_file(&p);
        Arc::new(DbManager::new_with_path(p))
    }

    #[test]
    fn timeout_defaults_to_thirty_minutes_and_is_configurable() {
        let db = db_at("timeout");
        assert_eq!(cron_timeout(&db), Duration::from_secs(30 * 60), "默认 30 分钟");

        let _ = db.set_setting("cron_timeout_minutes", "5");
        assert_eq!(cron_timeout(&db), Duration::from_secs(5 * 60));

        // 0 = 明确放弃这道保护，但不能变成 0 秒（那等于每次都立刻超时）
        let _ = db.set_setting("cron_timeout_minutes", "0");
        assert!(cron_timeout(&db) > Duration::from_secs(365 * 24 * 3600), "0 应表示不限制");

        // 填了非数字不能崩，回落默认
        let _ = db.set_setting("cron_timeout_minutes", "abc");
        assert_eq!(cron_timeout(&db), Duration::from_secs(30 * 60));
    }

    /// 陈旧的 running 行必须能被回收，否则一次卡死会把任务永久锁住——
    /// 保护措施把功能关掉，比没有保护更糟。
    #[tokio::test]
    async fn stale_running_rows_do_not_lock_a_task_forever() {
        let db = db_at("stale");
        {
            let conn = db.get_connection().unwrap();
            conn.execute(
                "INSERT INTO cron_runs (id, task_id, status, log_path, started_at)
                 VALUES ('old', 't1', 'running', '', datetime('now', '-3 hours'))",
                [],
            )
            .unwrap();
        }
        // 触发一次（agent 不存在会提前返回，但陈旧回收在那之前已经跑过）
        let _ = run_cron_task(
            Arc::clone(&db),
            "t1".into(),
            "不存在的Agent".into(),
            "[]".into(),
            std::env::temp_dir().to_string_lossy().to_string(),
        )
        .await;

        let conn = db.get_connection().unwrap();
        let still_running: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cron_runs WHERE id = 'old' AND status = 'running'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_running, 0, "三小时前的 running 应被收成 timeout");
    }

    /// 上一轮真的还在跑（刚开始）时，这一轮要跳过而不是叠加。
    #[tokio::test]
    async fn a_still_running_task_skips_instead_of_stacking() {
        let db = db_at("overlap");
        {
            let conn = db.get_connection().unwrap();
            conn.execute(
                "INSERT INTO cron_runs (id, task_id, status, log_path, started_at)
                 VALUES ('fresh', 't2', 'running', '', CURRENT_TIMESTAMP)",
                [],
            )
            .unwrap();
        }
        let err = run_cron_task(
            Arc::clone(&db),
            "t2".into(),
            "不存在的Agent".into(),
            "[]".into(),
            std::env::temp_dir().to_string_lossy().to_string(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("跳过"), "{err}");

        let conn = db.get_connection().unwrap();
        // 跳过要留痕，且用独立状态，不能混进 failed
        let skipped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cron_runs WHERE task_id = 't2' AND status = 'skipped'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(skipped, 1, "跳过要记一条 skipped");
    }
}

#[cfg(test)]
mod schedule_tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    /// `*/0` 会走到 `current % 0`——**整数除零，直接 panic**。这串东西以前存得
    /// 进库，然后每分钟把调度线程炸一次。现在解析阶段就不认它。
    #[test]
    fn a_zero_step_is_not_a_schedule() {
        assert_eq!(parse_schedule("*/0 * * * *"), None);
        // 真跑一遍：认得出来的那些一个都不能在求值时 panic。
        for good in ["*/15 * * * *", "* * * * *", "0 0 1 1 0"] {
            let parsed = parse_schedule(good).expect(good);
            let _ = parsed.is_due(at(2026, 8, 1, 9, 30), None);
        }
    }

    /// 标准 cron 的区间和列表这里**不支持**，必须认不出来而不是当成能跑。
    #[test]
    fn ranges_and_lists_are_not_silently_accepted() {
        for unsupported in ["0 9 * * 1-5", "0 9 * * 1,3,5", "0-30 * * * *"] {
            assert_eq!(parse_schedule(unsupported), None, "{unsupported}");
        }
    }

    /// 四种支持的写法各解析成什么。
    #[test]
    fn the_four_supported_forms_parse() {
        assert_eq!(parse_schedule("every 30 minutes"), Some(Schedule::EveryMinutes(30)));
        assert_eq!(parse_schedule("every 2 hours"), Some(Schedule::EveryHours(2)));
        assert_eq!(parse_schedule("Daily At 09:05"), Some(Schedule::DailyAt(9, 5)));
        assert_eq!(
            parse_schedule("*/15 * * * *"),
            Some(Schedule::Cron(vec![
                CronField::Step(15),
                CronField::Any,
                CronField::Any,
                CronField::Any,
                CronField::Any,
            ]))
        );
        // 时刻不合法的 `daily at` 不能掉下去当 cron 解析。
        assert_eq!(parse_schedule("daily at 25:00"), None);
        assert_eq!(parse_schedule("daily at 09:61"), None);
    }

    /// `daily at HH:MM` 一天只能触发一次：同一天已经跑过就不再跑，隔天才放行。
    #[test]
    fn daily_fires_once_a_day_at_the_stated_minute() {
        let s = parse_schedule("daily at 09:30").unwrap();
        let today = at(2026, 8, 1, 9, 30);
        assert!(s.is_due(today, None), "没跑过就该跑");
        assert!(!s.is_due(today, Some(at(2026, 8, 1, 9, 30))), "今天跑过了");
        assert!(s.is_due(today, Some(at(2026, 7, 31, 9, 30))), "昨天跑的，今天该跑");
        assert!(!s.is_due(at(2026, 8, 1, 9, 31), None), "差一分钟就不该跑");
    }

    /// `every N minutes` 要等够 N 分钟。
    #[test]
    fn interval_schedules_wait_out_the_interval() {
        let s = parse_schedule("every 30 minutes").unwrap();
        let now = at(2026, 8, 1, 12, 0);
        assert!(s.is_due(now, None));
        assert!(!s.is_due(now, Some(at(2026, 8, 1, 11, 45))), "只过了 15 分钟");
        assert!(s.is_due(now, Some(at(2026, 8, 1, 11, 30))));

        let h = parse_schedule("every 2 hours").unwrap();
        assert!(!h.is_due(now, Some(at(2026, 8, 1, 11, 0))));
        assert!(h.is_due(now, Some(at(2026, 8, 1, 10, 0))));
    }

    /// 五个字段各自对上自己那一位，不能串位。
    #[test]
    fn cron_fields_line_up_with_the_right_time_unit() {
        // 分 时 日 月 周 —— 2026-08-01 是星期六（num_days_from_sunday = 6）
        let s = parse_schedule("30 9 1 8 6").unwrap();
        assert!(s.is_due(at(2026, 8, 1, 9, 30), None));
        assert!(!s.is_due(at(2026, 8, 1, 9, 31), None), "分不对");
        assert!(!s.is_due(at(2026, 8, 1, 10, 30), None), "时不对");
        assert!(!s.is_due(at(2026, 8, 2, 9, 30), None), "日不对");
        assert!(!s.is_due(at(2026, 9, 1, 9, 30), None), "月不对");
    }
}

/// Q1′：把这次运行时间窗内观察到的动作摘要写进运行记录。
///
/// 用**时间窗**关联而不是运行 id：网关只知道是哪个 agent 在请求，不知道这是
/// 哪一次定时运行。窗口内如果同时开着手动会话，会一并算进来——已知的近似，
/// 所以摘要措辞是「这段时间内」而不是「这次运行」。
async fn summarize_run(db: &DbManager, run_id: &str, agent: &str, started_at: &str) {
    let end = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let Ok(summary) = crate::action_audit::summarize_window(db, agent, started_at, &end) else {
        return;
    };
    let line = summary.headline();
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute(
            "UPDATE cron_runs SET action_summary = ?1 WHERE id = ?2",
            params![line, run_id],
        );
    }
    // 对外动作是唯一值得单独喊一声的：它收不回来。
    if summary.send > 0 {
        log::warn!(
            "定时任务 {run_id}（{agent}）期间观察到 {} 次对外动作：{}",
            summary.send,
            summary
                .notable
                .iter()
                .filter(|a| a.risk_tier == "send")
                .map(|a| a.detail.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        );
    }
}

#[cfg(test)]
mod memory_injection_wiring {
    /// 记忆回注必须有一个真实的调用方。
    ///
    /// 这条守的是它自己犯过的错：`inject_workspace_memories` 原本挂在
    /// `AgentManager::spawn_agent` 里，而 spawn 的唯一入口 `start_agent_session`
    /// 没有任何调用方——于是「把记忆库写进工作区 CLAUDE.md / AGENTS.md」这个功能
    /// 静默停摆了。`build_memory_block` 一直是好的、`evolution.rs` 的文档也一直说
    /// 「回注由 inject_workspace_memories 负责」，测试全绿，只是没有任何东西再调它。
    ///
    /// 单元测试测的是这个函数本身写得对不对，测不出「没人调它」。所以在这里扫源码。
    #[test]
    fn inject_workspace_memories_has_a_live_caller() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut callers = Vec::new();
        for name in ["runtime_manager.rs", "runtime.rs", "lib.rs"] {
            let path = src.join(name);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if text.contains("inject_workspace_memories(") {
                callers.push(name);
            }
        }
        assert!(
            !callers.is_empty(),
            "没有任何运行时入口调用 inject_workspace_memories——记忆回注又断了。
             它应该挂在会话启动路径上（当前是 RuntimeManager::start_session）。"
        );
    }
}
