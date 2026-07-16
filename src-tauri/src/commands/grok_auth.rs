//! Grok 账号登录（认证中心）。
//!
//! 和 Claude/OpenAI/Gemini 那几个 provider 不同，这里**不**重新实现 xAI 的 OAuth，
//! 也不保存 xAI 的令牌。原因：Grok 自己就管凭据 —— `grok login` 跑 xAI 官方的
//! OAuth、把令牌写进 `~/.grok/auth.json`、并在后台自动续期（401 时也会自动刷新）。
//! 重实现一遍只会得到一份需要跟着 xAI 变的、猜出来的 client_id 和刷新逻辑。
//!
//! 所以 OMNIX 只做「转呈」：驱动 `grok login --device-auth`，把 xAI 打印的确认链接
//! 和设备码原样呈给用户。**密码和令牌全程不经 OMNIX。**
//!
//! 选设备码流（而非默认的浏览器流）是因为它对 GUI 宿主最可控：xAI 把链接和码打到
//! stderr，我们能确定地取出来给按钮用，不依赖子进程能不能自己拉起浏览器；同一套流程
//! 在远程/无浏览器环境（Labs）也照样成立。链接里已经带了 `?user_code=`，桌面端点一下
//! 就能确认，并不比浏览器流多一步。
//!
//! Grok 的凭据优先级（官方文档 02-authentication.md）：
//!   1. `[model.<name>]` 里的 api_key/env_key   2. `~/.grok/auth.json` 的会话令牌
//!   3. `XAI_API_KEY` 环境变量（兜底）
//! 所以「已登录」时会话令牌优先于 API Key —— 状态里两者都报给用户，避免困惑。

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;

use crate::agent::AgentManager;
use crate::db::DbManager;
use crate::proc::NoWindow;

#[derive(Debug, Clone, Serialize)]
pub struct GrokAuthStatus {
    pub cli_installed: bool,
    pub cli_path: Option<String>,
    /// `~/.grok/auth.json` exists and is non-empty (Grok owns this file).
    pub signed_in: bool,
    pub auth_file: String,
    /// `XAI_API_KEY` visible to this process (Grok's own fallback).
    pub api_key_env: bool,
    /// An xAI key is already saved in 模型中心 — usable without signing in.
    pub api_key_in_omnix: bool,
}

fn grok_auth_file() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".grok")
        .join("auth.json")
}

fn grok_exe(db: &DbManager) -> Result<String, String> {
    AgentManager::find_agent_path_static("Grok Build", Some(db))
        .ok_or_else(|| "没有找到 Grok Build CLI，请先到「智能体」页安装".to_string())
}

fn login_child() -> &'static Mutex<Option<Child>> {
    static CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    CHILD.get_or_init(|| Mutex::new(None))
}

/// Pull the sign-in URL and confirmation code out of Grok's own output.
///
/// Real output (v0.2.97, stderr, ANSI-coloured):
/// ```text
///   To sign in, open this URL in your browser:
///     https://accounts.x.ai/oauth2/device?user_code=BYQ7-ADW7
///   Confirm this code in your browser:
///     BYQ7-ADW7
/// ```
/// The raw lines are streamed to the UI regardless, so a parse miss degrades to
/// "user reads Grok's own text" rather than a dead end.
fn parse_login_prompt(buffer: &str) -> Option<(String, String)> {
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    static CODE: OnceLock<regex::Regex> = OnceLock::new();
    let url_re = URL.get_or_init(|| regex::Regex::new(r"https://[^\s'\x22]+").unwrap());
    let code_re = CODE.get_or_init(|| regex::Regex::new(r"\b[A-Z0-9]{4}-[A-Z0-9]{4}\b").unwrap());

    let url = url_re
        .find(buffer)?
        .as_str()
        .trim_end_matches(['.', ',', ')'])
        .to_string();
    // The code is also embedded in the URL's `user_code` query, so fall back to it.
    let code = code_re
        .find(buffer)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    Some((url, code))
}

#[tauri::command]
pub fn grok_auth_status(db: State<'_, Arc<DbManager>>) -> Result<GrokAuthStatus, String> {
    let cli_path = AgentManager::find_agent_path_static("Grok Build", Some(&db));
    let auth_file = grok_auth_file();
    let api_key_in_omnix = db
        .get_connection()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT api_key FROM model_platforms WHERE id = 'xai'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .is_some_and(|key| !key.trim().is_empty());

    Ok(GrokAuthStatus {
        cli_installed: cli_path.is_some(),
        cli_path,
        signed_in: std::fs::metadata(&auth_file).is_ok_and(|meta| meta.len() > 0),
        auth_file: auth_file.to_string_lossy().to_string(),
        api_key_env: std::env::var("XAI_API_KEY").is_ok_and(|v| !v.trim().is_empty()),
        api_key_in_omnix,
    })
}

/// Start `grok login --device-auth`. Streams Grok's output as `grok-login-output`,
/// emits `grok-login-prompt` {url, code} as soon as the link is known, and
/// `grok-login-done` {code, signed_in} when Grok finishes polling.
#[tauri::command]
pub async fn grok_login_start(
    app: AppHandle,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let exe = grok_exe(&db)?;
    {
        // Scoped: never hold a std Mutex guard across an await.
        let mut slot = login_child().lock().unwrap();
        if let Some(child) = slot.as_mut() {
            match child.try_wait() {
                Ok(None) => return Err("已有一个登录流程在进行中".into()),
                _ => *slot = None,
            }
        }
    }

    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(["login", "--device-auth"])
        .no_window()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("启动 grok login 失败: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    *login_child().lock().unwrap() = Some(child);

    // Grok prints the link and code on stderr; read both streams anyway.
    for (stream, is_err) in [
        (StreamKind::Out(stdout), false),
        (StreamKind::Err(stderr), true),
    ] {
        let app2 = app.clone();
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut prompted = false;
            match stream {
                StreamKind::Out(s) => {
                    let mut lines = BufReader::new(s).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        pump(&app2, &line, is_err, &mut buffer, &mut prompted);
                    }
                }
                StreamKind::Err(s) => {
                    let mut lines = BufReader::new(s).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        pump(&app2, &line, is_err, &mut buffer, &mut prompted);
                    }
                }
            }
        });
    }

    // Waiter: reap the child and report the outcome.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let done = {
                let mut slot = login_child().lock().unwrap();
                match slot.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
                        Ok(None) => None,
                        Err(_) => Some(-1),
                    },
                    None => Some(-2), // cancelled by the user
                }
            };
            if let Some(code) = done {
                *login_child().lock().unwrap() = None;
                let signed_in =
                    std::fs::metadata(grok_auth_file()).is_ok_and(|meta| meta.len() > 0);
                let _ = app.emit(
                    "grok-login-done",
                    serde_json::json!({ "code": code, "signed_in": signed_in }),
                );
                break;
            }
        }
    });
    Ok(())
}

enum StreamKind {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

/// Strip ANSI, forward the line to the UI, and emit the sign-in prompt once.
fn pump(app: &AppHandle, line: &str, is_err: bool, buffer: &mut String, prompted: &mut bool) {
    let clean = String::from_utf8_lossy(&strip_ansi_escapes::strip(line.as_bytes())).to_string();
    let _ = app.emit(
        "grok-login-output",
        serde_json::json!({ "line": clean, "stderr": is_err }),
    );
    if *prompted {
        return;
    }
    buffer.push_str(&clean);
    buffer.push('\n');
    if let Some((url, code)) = parse_login_prompt(buffer) {
        *prompted = true;
        let _ = app.emit("grok-login-prompt", serde_json::json!({ "url": url, "code": code }));
    }
}

#[tauri::command]
pub async fn grok_login_cancel() -> Result<(), String> {
    let child = login_child().lock().unwrap().take();
    if let Some(mut child) = child {
        let _ = child.kill().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn grok_logout(db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let exe = grok_exe(&db)?;
    let output = tokio::process::Command::new(exe)
        .arg("logout")
        .no_window()
        .output()
        .await
        .map_err(|e| format!("启动 grok logout 失败: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "grok logout 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_login_prompt;

    /// Verbatim stderr from `grok login --device-auth` (v0.2.97), ANSI included.
    const REAL_OUTPUT: &str = "\nTo sign in, open this URL in your browser:\n\n  \
        https://accounts.x.ai/oauth2/device?user_code=BYQ7-ADW7\n\n\
        Confirm this code in your browser:\n\n  BYQ7-ADW7\n\n\
        \u{1b}[90mOnly continue with a code you requested. Don't share it with anyone.\u{1b}[0m\n\n\
        Waiting for authorization...\n";

    #[test]
    fn parses_real_device_auth_output() {
        let (url, code) = parse_login_prompt(REAL_OUTPUT).expect("prompt parsed");
        assert_eq!(url, "https://accounts.x.ai/oauth2/device?user_code=BYQ7-ADW7");
        assert_eq!(code, "BYQ7-ADW7");
    }

    #[test]
    fn waits_until_the_url_arrives() {
        // Partial output (Grok hasn't printed the link yet) must not prompt early.
        assert!(parse_login_prompt("\nTo sign in, open this URL in your browser:\n").is_none());
    }

    #[test]
    fn rate_limit_error_is_not_mistaken_for_a_prompt() {
        // Observed when the device endpoint throttles; carries no https link.
        let err = "Error: Device code request failed (HTTP 429 Too Many Requests): \
                   {\"error\":\"slow_down\",\"error_description\":\"Too many device code requests.\"}";
        assert!(parse_login_prompt(err).is_none());
    }
}
