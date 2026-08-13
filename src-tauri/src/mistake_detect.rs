//! 失误检测：从命令输出里认出编译错误、测试失败、崩溃、泄密等信号。
//!
//! **这个功能之前整段没运行过。** 它原本是前端 `lib/terminal.ts` 里的
//! `detectMistakes`，挂在 PTY 的 `agent-output` 监听上；而 PTY 会话根本建不出来
//! （`start_agent_session` 没有任何调用方），所以 `agent-output` 永远不发。
//! `build_memory_block` 那边的记忆回注是同一条链上一起停摆的。
//!
//! 搬到 Rust 有两个实打实的好处，不只是「换个地方」：
//!
//! 1. **不开界面也检测。** 原来的版本只有主窗口在渲染时才跑；agent 在后台跑一夜，
//!    什么都不会记下来。
//! 2. **信号更干净。** 原来扫的是原始终端流（要先剥 ANSI、还会扫到 agent 自己
//!    复述错误的话）；现在挂在 `tool_completed` 事件上，那是一条命令执行完的输出，
//!    有边界、不含对话文本。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbManager;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedMistake {
    pub category: String,
    pub severity: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub raw_line: String,
}

/// 一条模式。`kind` 决定怎么组织 message，避免为每条写一个闭包。
struct Pattern {
    re: &'static str,
    category: &'static str,
    severity: &'static str,
    /// message 的组织方式：整行、某个捕获组、或固定文案。
    render: Render,
}

enum Render {
    /// 用第 n 个捕获组
    Group(usize),
    /// 用整个匹配
    Whole,
    /// 固定文案（泄密这类，原文不该抄进日志）
    Fixed(&'static str),
    /// `前缀 + 组n: 组m`
    Pair(&'static str, usize, usize),
}

fn patterns() -> &'static [Pattern] {
    // 顺序有意义：先匹配的赢，所以具体的排在笼统的前面
    // （`error[E0308]` 必须排在 `error:` 前面）。
    &[
        // ── 编译错误 ──
        Pattern { re: r"^error\[E(\d{4})\]:\s*(.+)", category: "compile_error", severity: "high",
                  render: Render::Pair("Rust E", 1, 2) },
        Pattern { re: r"^(.+\.ts\(\d+,\d+\)):\s*error TS(\d+):\s*(.+)", category: "compile_error", severity: "high",
                  render: Render::Pair("TS", 2, 3) },
        Pattern { re: r"error TS(\d+):\s*(.+)", category: "compile_error", severity: "high",
                  render: Render::Pair("TS", 1, 2) },
        Pattern { re: r#"Cannot find name\s+['"](.+?)['"]"#, category: "compile_error", severity: "high",
                  render: Render::Whole },
        Pattern { re: r"^error:\s*(.+)", category: "compile_error", severity: "high",
                  render: Render::Group(1) },
        // ── 测试失败 ──
        Pattern { re: r"^FAILED\s+(.+)", category: "test_failure", severity: "high",
                  render: Render::Group(1) },
        Pattern { re: r"(\d+)\s+tests?\s+failed", category: "test_failure", severity: "high",
                  render: Render::Whole },
        Pattern { re: r"(?i)panicked at|panic!", category: "test_failure", severity: "high",
                  render: Render::Whole },
        Pattern { re: r"AssertionError", category: "test_failure", severity: "high",
                  render: Render::Whole },
        // ── 运行时崩溃 ──
        Pattern { re: r"(?i)SIGSEGV|segmentation fault", category: "runtime_crash", severity: "high",
                  render: Render::Whole },
        Pattern { re: r"Uncaught\s+(TypeError|ReferenceError|RangeError|SyntaxError):\s*(.+)",
                  category: "runtime_crash", severity: "high", render: Render::Pair("", 1, 2) },
        // ── API 错误 ──
        Pattern { re: r"(429|401|403|500|502|503)\s+(Too Many Requests|Unauthorized|Forbidden|Internal Server Error|Bad Gateway|Service Unavailable)",
                  category: "api_error", severity: "medium", render: Render::Whole },
        Pattern { re: r"(?i)API key invalid|Invalid API key|incorrect api key",
                  category: "api_error", severity: "high", render: Render::Whole },
        // ── 泄密 ──
        // 这几条**不把原文写进日志**——活动日志本身会被翻看、导出、进备份。
        Pattern { re: r#"(?i)api_?key\s*[=:]\s*["']sk-"#, category: "privacy_leak", severity: "high",
                  render: Render::Fixed("疑似硬编码 API Key") },
        Pattern { re: r#"(?i)password\s*[=:]\s*["'][^"']{3,}"#, category: "privacy_leak", severity: "high",
                  render: Render::Fixed("疑似硬编码密码") },
        Pattern { re: r#"(?i)secret\s*[=:]\s*["'][^"']{8,}"#, category: "privacy_leak", severity: "high",
                  render: Render::Fixed("疑似硬编码密钥") },
        Pattern { re: r"(?i)Bearer\s+sk-", category: "privacy_leak", severity: "high",
                  render: Render::Fixed("疑似泄露的 Bearer 令牌") },
        // ── lint（最笼统，排最后）──
        Pattern { re: r"^warning:\s*(.+)", category: "lint_warning", severity: "medium",
                  render: Render::Group(1) },
    ]
}

/// 每行最多认一条（第一个命中的模式赢），整体去重。
pub fn detect(text: &str) -> Vec<DetectedMistake> {
    let compiled: Vec<(regex::Regex, &Pattern)> = patterns()
        .iter()
        .filter_map(|p| regex::Regex::new(p.re).ok().map(|re| (re, p)))
        .collect();
    let file_re = regex::Regex::new(r"(\S+\.\w+):(\d+)").ok();

    let mut out: Vec<DetectedMistake> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() < 5 {
            continue;
        }
        for (re, p) in &compiled {
            let Some(caps) = re.captures(trimmed) else {
                continue;
            };
            let grp = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("").to_string();
            let message = match p.render {
                Render::Group(i) => grp(i),
                Render::Whole => caps.get(0).map(|m| m.as_str()).unwrap_or(trimmed).to_string(),
                Render::Fixed(s) => s.to_string(),
                Render::Pair(prefix, a, b) => format!("{prefix}{}: {}", grp(a), grp(b)),
            };
            // 泄密那几条连原文都不留——日志会被翻看/导出/进备份。
            let raw_line = if p.category == "privacy_leak" {
                String::from("（已隐去：命中泄密模式的原文不写入日志）")
            } else {
                trimmed.chars().take(300).collect()
            };
            let (file, line_no) = file_re
                .as_ref()
                .and_then(|fr| fr.captures(trimmed))
                .filter(|_| p.category != "privacy_leak")
                .map(|c| {
                    (
                        c.get(1).map(|m| m.as_str().to_string()),
                        c.get(2).and_then(|m| m.as_str().parse::<u32>().ok()),
                    )
                })
                .unwrap_or((None, None));

            let mistake = DetectedMistake {
                category: p.category.to_string(),
                severity: p.severity.to_string(),
                message: message.chars().take(200).collect(),
                file,
                line: line_no,
                raw_line,
            };
            if !out.contains(&mistake) {
                out.push(mistake);
            }
            break; // 一行只认一条
        }
    }
    out
}

/// 检测一次并写进 `activity_log`。没有命中就什么都不做（不留空记录）。
///
/// 失败只告警：失误检测是旁路观察，不该让一次工具调用的记录写不进去。
pub fn detect_and_log(db: &DbManager, session_id: &str, text: &str) {
    let found = detect(text);
    if found.is_empty() {
        return;
    }
    let Ok(conn) = db.get_connection() else {
        return;
    };
    let details = match serde_json::to_string(&found) {
        Ok(s) => s,
        Err(_) => return,
    };
    let id = format!("act_{}_{}", chrono::Utc::now().timestamp_micros(), session_id);
    if let Err(error) = conn.execute(
        "INSERT INTO activity_log (id, action, target, details) VALUES (?1, 'mistake_detected', ?2, ?3)",
        params![id, session_id, details],
    ) {
        log::warn!("失误检测写日志失败（不影响本次调用）：{error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(String, String)> {
        detect(text)
            .into_iter()
            .map(|m| (m.category, m.message))
            .collect()
    }

    /// 必须有运行时入口在调它。
    ///
    /// 这条守的正是它自己的死法：检测逻辑一直好好的、单测也全绿，只是唯一的
    /// 调用方挂在一条不可达的路径（PTY）上，于是整段静默停摆。单测测的是
    /// 「认得准不准」，测不出「有没有人调」——只能扫源码。
    /// 同 `agent::memory_injection_wiring`。
    #[test]
    fn detect_and_log_has_a_live_caller() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let callers: Vec<&str> = ["runtime_manager.rs", "runtime.rs", "lib.rs"]
            .into_iter()
            .filter(|name| {
                std::fs::read_to_string(src.join(name))
                    .map(|text| text.contains("mistake_detect::detect_and_log("))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            !callers.is_empty(),
            "没有任何运行时入口调用 detect_and_log——失误检测又断了。
             它应该挂在事件落库路径上（当前是 persist_runtime_event 里的 ToolCompleted 分支）。"
        );
    }

    #[test]
    fn recognises_rust_and_ts_compile_errors() {
        let found = detect("error[E0308]: mismatched types\nsrc/a.ts(12,3): error TS2345: bad arg");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].category, "compile_error");
        assert!(found[0].message.starts_with("Rust E0308:"), "{}", found[0].message);
        assert!(found[1].message.starts_with("TS2345:"), "{}", found[1].message);
    }

    /// 具体模式要排在笼统模式前面，否则 `error[E0308]` 会被 `^error:` 吃掉，
    /// 丢掉错误码。
    #[test]
    fn specific_patterns_win_over_generic_ones() {
        let found = detect("error[E0425]: cannot find value");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("E0425"), "{}", found[0].message);
    }

    #[test]
    fn recognises_test_failures_and_panics() {
        let cats: Vec<String> = detect(
            "thread 'x' panicked at src/lib.rs:42:9\n3 tests failed\nFAILED tests::foo",
        )
        .into_iter()
        .map(|m| m.category)
        .collect();
        assert_eq!(cats, vec!["test_failure", "test_failure", "test_failure"]);
    }

    #[test]
    fn extracts_file_and_line_when_present() {
        let found = detect("thread 'x' panicked at src/lib.rs:42:9");
        assert_eq!(found[0].file.as_deref(), Some("src/lib.rs"));
        assert_eq!(found[0].line, Some(42));
    }

    /// 泄密那几条的原文**不能**进日志——活动日志会被翻看、导出、进备份。
    #[test]
    fn privacy_hits_never_carry_the_secret() {
        let found = detect(r#"api_key = "sk-ABCDEF1234567890""#);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].category, "privacy_leak");
        assert!(!found[0].raw_line.contains("sk-"), "原文泄进了 raw_line");
        assert!(!found[0].message.contains("sk-"), "原文泄进了 message");
    }

    #[test]
    fn clean_output_produces_nothing() {
        assert!(detect("   Compiling omnix v0.1.0\n    Finished in 3.2s\nok").is_empty());
        assert!(detect("test result: ok. 12 passed; 0 failed").is_empty());
    }

    /// 同一条命令的输出里重复出现同一个错误，只记一次。
    #[test]
    fn repeats_within_one_output_are_deduped() {
        let found = detect("error: boom\nerror: boom\nerror: boom");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn short_lines_are_ignored() {
        assert!(kinds("err\nok\nx").is_empty());
    }
}
