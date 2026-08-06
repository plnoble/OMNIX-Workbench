//! Q2′ 事后审计：无人值守跑完之后，看得见它到底做了什么。
//!
//! ## 为什么是「事后」不是「拦截」
//!
//! OMNIX 把 agent CLI 当子进程拉起来，工具审批由那些 CLI 自己在内部处理——
//! OMNIX 不在那个决策环里。要做拦截式审批，得在网关里拦住响应、等人批完再放行，
//! 那会打断流式输出，还要跟每家 CLI 自己的审批机制打架。
//!
//! 但 OMNIX **看得见**：所有 agent 的模型请求都过网关，而请求里带着上一轮的
//! `tool_use` 块。也就是说，agent 每调用一次工具，下一次请求就会把它带过来。
//! 于是不用拦截也能记全——晚一轮，但一条不漏。
//!
//! （这条路能走通，前提是网关不再把 tool_use 的字段吃掉。见 proxy_types 的
//! `wire_fidelity_tests`——修好那个 bug 之前，这里什么也看不到。）
//!
//! ## 去重
//!
//! 对话历史每一轮都会把之前**所有**的 tool_use 重发一遍。不去重的话，一次
//! 20 轮的会话会把第一个工具调用记 20 次。`tool_use` 自带唯一 id，拿它当主键，
//! `INSERT OR IGNORE` 天然幂等。

use rusqlite::params;
use serde::Serialize;

use crate::db::DbManager;

/// 动作的风险档。分档的意义是让「跑了一夜都干了啥」这个问题有个能一眼看完的答案。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// 只读：看文件、搜索、列目录
    Read,
    /// 改本机文件
    Write,
    /// 执行命令
    Execute,
    /// **对外且不可撤销**：推代码、发消息、开 PR、部署
    Send,
}

impl RiskTier {
    pub fn as_str(self) -> &'static str {
        match self {
            RiskTier::Read => "read",
            RiskTier::Write => "write",
            RiskTier::Execute => "execute",
            RiskTier::Send => "send",
        }
    }
}

/// 命令行里出现这些，就不再是「本机执行」而是「对外且收不回来」。
/// 保守取舍：宁可把普通命令误判成 send（多看一眼），也不能把真的推送漏成 execute。
const OUTBOUND_MARKERS: &[&str] = &[
    "git push", "gh pr create", "gh release", "npm publish", "cargo publish",
    "docker push", "kubectl apply", "terraform apply", "aws ", "scp ", "rsync ",
    "curl -x post", "curl --request post", "mail ", "sendmail",
];

/// 按工具名与参数判风险档。
///
/// 工具名认不出来时按 `Execute` 处理而不是 `Read`——不认识的东西当成危险的，
/// 是这类判断唯一安全的默认方向。
pub fn classify(tool_name: &str, input: &serde_json::Value) -> RiskTier {
    let name = tool_name.to_ascii_lowercase();
    match name.as_str() {
        "read" | "glob" | "grep" | "ls" | "notebookread" | "webfetch" | "websearch"
        | "todoread" | "list" => RiskTier::Read,
        "write" | "edit" | "multiedit" | "notebookedit" | "todowrite" => RiskTier::Write,
        "bash" | "shell" | "run" | "execute" | "terminal" => {
            // shell 命令得看内容：`git push` 跟 `ls` 不是一回事。
            let cmd = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if OUTBOUND_MARKERS.iter().any(|m| cmd.contains(m)) {
                RiskTier::Send
            } else {
                RiskTier::Execute
            }
        }
        _ => RiskTier::Execute,
    }
}

/// 从一个 Anthropic 请求体里摘出 agent 已经调用过的工具。
///
/// 读的是**历史里的** tool_use 块，所以拿到的是上一轮（及更早）真实发生过的调用，
/// 不是模型打算做什么。审计要的正是前者。
pub fn extract_tool_uses(
    payload: &crate::proxy_types::AnthropicRequest,
) -> Vec<(String, String, serde_json::Value)> {
    let mut out = Vec::new();
    for msg in &payload.messages {
        let crate::proxy_types::AnthropicMessageContent::Blocks(blocks) = &msg.content else {
            continue;
        };
        for b in blocks {
            if b.block_type != "tool_use" {
                continue;
            }
            // id/name/input 都躺在 extra 里（结构体只显式认 type/text/source）
            let id = b.extra.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let name = b.extra.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() || name.is_empty() {
                continue;
            }
            let input = b.extra.get("input").cloned().unwrap_or(serde_json::Value::Null);
            out.push((id.to_string(), name.to_string(), input));
        }
    }
    out
}

/// 记一批动作。以 tool_use id 为主键，重复提交自动忽略。
/// 审计失败绝不能影响正在转发的请求——所有错误在这里咽掉。
pub fn record(db: &DbManager, agent: &str, uses: &[(String, String, serde_json::Value)]) {
    if uses.is_empty() {
        return;
    }
    let Ok(conn) = db.get_connection() else { return };
    for (id, name, input) in uses {
        let tier = classify(name, input);
        // 只留一小段参数摘要：审计是为了「看得见做了什么」，不是把整个会话抄一份。
        let detail = match input.get("command").and_then(|v| v.as_str()) {
            Some(c) => c.chars().take(200).collect::<String>(),
            None => input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>(),
        };
        let _ = conn.execute(
            "INSERT OR IGNORE INTO agent_actions (id, agent, tool_name, risk_tier, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, agent, name, tier.as_str(), detail],
        );
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ActionSummary {
    pub read: i64,
    pub write: i64,
    pub execute: i64,
    pub send: i64,
    /// 高风险动作的明细（execute / send），给人过目
    pub notable: Vec<NotableAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotableAction {
    pub tool_name: String,
    pub risk_tier: String,
    pub detail: String,
    pub created_at: String,
}

impl ActionSummary {
    /// 一句话摘要，直接进定时任务的运行记录。
    pub fn headline(&self) -> String {
        if self.read + self.write + self.execute + self.send == 0 {
            return "未观察到工具调用".to_string();
        }
        let mut parts = Vec::new();
        if self.read > 0 { parts.push(format!("读 {}", self.read)); }
        if self.write > 0 { parts.push(format!("改文件 {}", self.write)); }
        if self.execute > 0 { parts.push(format!("执行 {}", self.execute)); }
        if self.send > 0 { parts.push(format!("**对外 {}**", self.send)); }
        parts.join(" · ")
    }
}

/// 某个时间窗内某 agent 做过的事。
///
/// 用时间窗关联而不是运行 id：网关只知道是哪个 agent 在请求，不知道这是哪一次
/// 定时运行。窗口内如果同时有手动会话，会一并算进来——这是已知的近似，
/// 所以界面上要说清是「这段时间内」而不是「这次运行」。
pub fn summarize_window(
    db: &DbManager,
    agent: &str,
    start: &str,
    end: &str,
) -> Result<ActionSummary, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut s = ActionSummary::default();
    let mut stmt = conn
        .prepare(
            "SELECT risk_tier, COUNT(*) FROM agent_actions
             WHERE agent = ?1 AND created_at >= ?2 AND created_at <= ?3
             GROUP BY risk_tier",
        )
        .map_err(|e| e.to_string())?;
    for row in stmt
        .query_map(params![agent, start, end], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?
        .flatten()
    {
        match row.0.as_str() {
            "read" => s.read = row.1,
            "write" => s.write = row.1,
            "execute" => s.execute = row.1,
            "send" => s.send = row.1,
            _ => {}
        }
    }
    drop(stmt);

    let mut stmt = conn
        .prepare(
            "SELECT tool_name, risk_tier, detail, created_at FROM agent_actions
             WHERE agent = ?1 AND created_at >= ?2 AND created_at <= ?3
               AND risk_tier IN ('execute','send')
             ORDER BY risk_tier = 'send' DESC, created_at LIMIT 50",
        )
        .map_err(|e| e.to_string())?;
    s.notable = stmt
        .query_map(params![agent, start, end], |r| {
            Ok(NotableAction {
                tool_name: r.get(0)?,
                risk_tier: r.get(1)?,
                detail: r.get(2)?,
                created_at: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(s)
}

/// T1：蒸馏用的证据文本——这段时间里 agent **真的**调用过哪些工具。
///
/// 借鉴 GenericAgent 的第一公理「无行动，不记忆」：
///
/// > 任何写入记忆的信息，必须源自成功的工具调用结果。严禁将模型的"固有知识"、
/// > "推理猜测"、"未执行的计划"或"未验证的假设"作为事实写入。
///
/// 蒸馏原来只喂三样：会话 transcript、协议事件、`.omx/development` 记录。
/// **三样都是「谁说过什么」**——transcript 里 agent 说"我改好了并测过了"，
/// 和它真的跑过 `cargo test` 在文本上分不出来。这个台账是网关侧记录的
/// 真实 `tool_use` 调用，是唯一能回答「到底做没做」的一手材料。
///
/// 空窗口返回空串：没观察到就如实说没有，绝不能让蒸馏器以为「没记录 = 没做过」
/// 或反过来编一个出来。
pub fn evidence_block(db: &DbManager, agent: &str, start: &str, end: &str) -> String {
    let Ok(conn) = db.get_connection() else {
        return String::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT created_at, tool_name, risk_tier, detail FROM agent_actions
         WHERE agent = ?1 AND created_at >= ?2 AND created_at <= ?3
         ORDER BY created_at ASC LIMIT 200",
    ) else {
        return String::new();
    };
    let lines: Vec<String> = stmt
        .query_map(params![agent, start, end], |r| {
            let at: String = r.get(0)?;
            let tool: String = r.get(1)?;
            let tier: String = r.get(2)?;
            let detail: String = r.get(3)?;
            Ok(if detail.trim().is_empty() {
                format!("- [{at}] {tool}（{tier}）")
            } else {
                format!("- [{at}] {tool}（{tier}）：{detail}")
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();

    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\n\n## 已核实的动作台账（网关侧记录的真实工具调用）\n\
         这一节是**做过什么**的一手证据，其余材料只是**说过什么**。\n\
         只有能对上这里某条记录的结论，才算「行动验证过」。\n\n{}\n",
        lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_types::AnthropicRequest;

    fn db(tag: &str) -> DbManager {
        let p = std::env::temp_dir().join(format!("omnix_audit_{}_{tag}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        DbManager::new_with_path(p)
    }

    #[test]
    fn shell_commands_are_judged_by_what_they_do() {
        let bash = |c: &str| classify("Bash", &serde_json::json!({ "command": c }));
        assert_eq!(bash("ls -la"), RiskTier::Execute);
        assert_eq!(bash("npm test"), RiskTier::Execute);
        // 对外且收不回来的，必须单独一档
        assert_eq!(bash("git push origin master"), RiskTier::Send);
        assert_eq!(bash("gh pr create --title x"), RiskTier::Send);
        assert_eq!(bash("npm publish"), RiskTier::Send);
        // 大小写不该影响判断
        assert_eq!(bash("GIT PUSH origin main"), RiskTier::Send);
    }

    #[test]
    fn unknown_tools_default_to_dangerous_not_safe() {
        assert_eq!(
            classify("某个没见过的工具", &serde_json::json!({})),
            RiskTier::Execute,
            "认不出来的要当成危险的——这类判断只有这个方向是安全的"
        );
        assert_eq!(classify("Read", &serde_json::json!({})), RiskTier::Read);
        assert_eq!(classify("Write", &serde_json::json!({})), RiskTier::Write);
    }

    #[test]
    fn tool_uses_are_extracted_from_the_conversation_history() {
        let raw = serde_json::json!({
            "model": "m", "messages": [
                { "role": "user", "content": "跑一下" },
                { "role": "assistant", "content": [
                    {"type": "text", "text": "好"},
                    {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "git push"}}
                ]},
                { "role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1"}] },
                { "role": "assistant", "content": [
                    {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "a.rs"}}
                ]}
            ]
        });
        let req: AnthropicRequest = serde_json::from_value(raw).unwrap();
        let uses = extract_tool_uses(&req);
        assert_eq!(uses.len(), 2, "两次调用都要摘出来: {uses:?}");
        assert_eq!(uses[0].1, "Bash");
        assert_eq!(uses[1].1, "Read");
    }

    /// 对话历史每轮都会重发之前所有的 tool_use。不去重的话一次会话能记几十遍。
    #[test]
    fn replayed_history_does_not_double_count() {
        let d = db("dedupe");
        let uses = vec![
            ("t1".into(), "Bash".into(), serde_json::json!({"command": "ls"})),
            ("t2".into(), "Write".into(), serde_json::json!({"file_path": "a.rs"})),
        ];
        // 模拟连续三轮，历史被重发三次
        record(&d, "Claude Code", &uses);
        record(&d, "Claude Code", &uses);
        record(&d, "Claude Code", &uses);

        let conn = d.get_connection().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_actions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2, "同一个 tool_use id 只该记一次");
    }

    #[test]
    fn summary_headline_calls_out_outbound_actions() {
        let d = db("summary");
        record(
            &d,
            "A",
            &[
                ("a".into(), "Read".into(), serde_json::json!({})),
                ("b".into(), "Bash".into(), serde_json::json!({"command": "cargo test"})),
                ("c".into(), "Bash".into(), serde_json::json!({"command": "git push origin x"})),
            ],
        );
        let s = summarize_window(&d, "A", "2000-01-01", "2999-01-01").unwrap();
        assert_eq!(s.read, 1);
        assert_eq!(s.execute, 1);
        assert_eq!(s.send, 1);
        let h = s.headline();
        assert!(h.contains("对外 1"), "对外动作必须在摘要里显眼: {h}");
        // 高风险明细要能过目，且对外的排前面
        assert_eq!(s.notable.len(), 2);
        assert_eq!(s.notable[0].risk_tier, "send");
        assert!(s.notable[0].detail.contains("git push"));
    }

    #[test]
    fn nothing_observed_says_so_instead_of_looking_clean() {
        let d = db("empty");
        let s = summarize_window(&d, "A", "2000-01-01", "2999-01-01").unwrap();
        assert_eq!(s.headline(), "未观察到工具调用", "没数据不能显示成「很安全」");
    }

    #[test]
    fn evidence_block_reports_what_was_actually_done() {
        let d = db("evidence");
        record(
            &d,
            "A",
            &[
                ("a".into(), "Edit".into(), serde_json::json!({"file_path": "src/proxy.rs"})),
                ("b".into(), "Bash".into(), serde_json::json!({"command": "cargo test --lib"})),
            ],
        );
        let block = evidence_block(&d, "A", "2000-01-01", "2999-01-01");
        assert!(block.contains("cargo test --lib"), "跑过的命令要出现在证据里: {block}");
        assert!(block.contains("src/proxy.rs"));
        // 蒸馏器必须知道这一节和其余材料的性质不同。
        assert!(block.contains("一手证据"), "要说清这节是「做过什么」: {block}");
    }

    #[test]
    fn no_observed_actions_yields_no_evidence_section_at_all() {
        // 空窗口给空串，而不是一个写着「无」的小节——后者会让蒸馏器
        // 把「没记录」读成「确认什么都没做」，反而更容易编出结论。
        let d = db("evidence_empty");
        assert!(evidence_block(&d, "A", "2000-01-01", "2999-01-01").is_empty());
    }
}
