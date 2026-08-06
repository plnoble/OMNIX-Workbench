//! P1：把 OMNIX 的能力对外暴露成一个 MCP 服务器。
//!
//! 动机一句话：**技能不该只活在 OMNIX 里。** 你平时在 Claude Code、Codex、Cursor
//! 里干活，攒下来的技能却锁在另一个应用里，等于每次都要先切换工具。挂上这个
//! MCP 之后，技能跟着人走。
//!
//! ## 只有两个工具，是刻意的
//!
//! 一个搜、一个取。工具越多，调用方的模型越容易挑错——把 20 个技能各暴露成一个
//! 工具，等于把选择困难塞给对面。搜索让模型用自然语言描述它想干什么，取回来的是
//! 完整的技能正文，接下来照着做就行。
//!
//! ## 为什么是「取」而不是「执行」
//!
//! OMNIX 的技能是**给模型看的说明书**（SKILL.md），不是可以服务端跑的函数。
//! 所以这里老老实实叫 `load_capability`：把说明书交出去，由调用方的 agent 执行。
//! 叫 `execute_capability` 会让对面以为我们替它做了事。
//!
//! ## 传输
//!
//! JSON-RPC 2.0 over HTTP POST，挂在网关的 `/mcp`。鉴权沿用网关那一套
//! （`proxy::guard_gateway_access`：本机直接放行，远程必须带令牌），这里不另立门户。

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::DbManager;

/// 我们实现的 MCP 协议版本。写死一个已知值比跟着对方回声要诚实——
/// 对面报一个我们没实现的版本时，它应该看到我们真正支持的那个。
const PROTOCOL_VERSION: &str = "2025-06-18";

const SEARCH_TOOL: &str = "search_capabilities";
const LOAD_TOOL: &str = "load_capability";
const DECK_TOOL: &str = "create_deck";
/// T4：团队任务板。看板 + 上报，让跑起来的 agent 参与协作而不只是被编排。
const BOARD_TOOL: &str = "team_board";
const REPORT_TOOL: &str = "team_report";

/// 搜索一次最多回几条。给对面塞 50 条技能只会挤爆它的上下文。
const MAX_RESULTS: usize = 8;

// ─────────────────────────────────────────────────────────────────────────
// JSON-RPC 信封
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[allow(dead_code)]
    #[serde(default)]
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    /// 没有 id = 通知（notification），按规范**不能**回响应
    #[serde(default)]
    pub id: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

fn ok(id: Value, result: Value) -> RpcResponse {
    RpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
}

fn err(id: Value, code: i32, message: impl Into<String>) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message: message.into() }),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 工具定义
// ─────────────────────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!([
        {
            "name": SEARCH_TOOL,
            "description":
                "在 OMNIX 技能库里按自然语言查找可用能力。先用它找到合适的技能，\
                 再用 load_capability 取回完整说明。描述你想完成什么，而不是猜技能名。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "你想完成的事，例如「把 Excel 里的数据做成图表」"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": DECK_TOOL,
            "description":
                "把你写好的幻灯内容渲染成**真正的文件**（.pptx + .html），返回文件路径。                 你负责内容，OMNIX 负责排版和导出——不需要你懂版式细节。                 常用 layout：cover 封面 / bullets 要点 / metrics 指标卡 / chart 图表 /                  swot 分析模型 / compare-table 对比表 / quote 引用 / section 章节页。                 返回里会附带体检结果（内容是否会溢出、图表有没有数据等）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "演示标题，也用作文件名" },
                    "theme": {
                        "type": "string",
                        "description": "midnight 午夜蓝 / minimal 极简白 / corporate 商务蓝 / sunset 落日紫",
                    },
                    "slides": {
                        "type": "array",
                        "description":
                            "每页一个对象：{layout, title, subtitle, bullets[], body,                              columns[{title,bullets[]}], items[{label,value,detail,group}], notes}",
                        "items": { "type": "object" }
                    }
                },
                "required": ["title", "slides"]
            }
        },
        {
            "name": BOARD_TOOL,
            "description":
                "看这次团队协作的任务板：有哪些分工、各自什么状态、谁卡在谁后面。                 开工前先看一眼——你依赖的分工没完成时不要动手。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "团队运行 id" }
                },
                "required": ["run_id"]
            }
        },
        {
            "name": REPORT_TOOL,
            "description":
                "上报你负责的那条分工的状态。做完、失败、或被别的分工卡住时都要报——                 队友靠这个判断能不能开工。只改你指定的那一条，不影响别人。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "团队运行 id" },
                    "assignment_id": { "type": "string", "description": "分工 id，从 team_board 拿" },
                    "status": {
                        "type": "string",
                        "description": "running 进行中 / completed 已完成 / failed 失败 / blocked 被卡住"
                    },
                    "note": { "type": "string", "description": "一句话进展或失败原因（可选）" }
                },
                "required": ["run_id", "assignment_id", "status"]
            }
        },
        {
            "name": LOAD_TOOL,
            "description":
                "取回一个技能的完整说明（SKILL.md 正文），取回后照着执行。\
                 技能名要用 search_capabilities 返回的 name。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "技能名" }
                },
                "required": ["name"]
            }
        }
    ])
}

// ─────────────────────────────────────────────────────────────────────────
// 分发
// ─────────────────────────────────────────────────────────────────────────

/// 处理一条 JSON-RPC 请求。返回 `None` 表示这是通知，按规范不回响应。
pub fn handle_rpc(db: &Arc<DbManager>, req: RpcRequest) -> Option<RpcResponse> {
    // 通知没有 id，不能回。最常见的是 initialized / cancelled。
    let Some(id) = req.id.clone() else {
        return None;
    };

    Some(match req.method.as_str() {
        "initialize" => ok(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "omnix",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        ),
        "tools/list" => ok(id, json!({ "tools": tool_definitions() })),
        "tools/call" => match call_tool(db, &req.params) {
            Ok(text) => ok(id, tool_text(&text, false)),
            // 工具出错走 isError，不是协议层错误——协议层报错会让对面把
            // 整个连接当成坏的，而这里只是这一次调用没成功。
            Err(e) => ok(id, tool_text(&e, true)),
        },
        "ping" => ok(id, json!({})),
        other => err(id, -32601, format!("未实现的方法: {other}")),
    })
}

fn tool_text(text: &str, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

fn call_tool(db: &Arc<DbManager>, params: &Value) -> Result<String, String> {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    match name {
        SEARCH_TOOL => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("").trim();
            if q.is_empty() {
                return Err("query 不能为空——描述一下你想完成什么。".into());
            }
            search_capabilities(db, q)
        }
        LOAD_TOOL => {
            let n = args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
            if n.is_empty() {
                return Err("name 不能为空。".into());
            }
            load_capability(db, n)
        }
        DECK_TOOL => create_deck(&args),
        BOARD_TOOL => {
            let run = args.get("run_id").and_then(|v| v.as_str()).unwrap_or("").trim();
            crate::team_board::board(db, run)
        }
        REPORT_TOOL => crate::team_board::report(
            db,
            args.get("run_id").and_then(|v| v.as_str()).unwrap_or("").trim(),
            args.get("assignment_id").and_then(|v| v.as_str()).unwrap_or("").trim(),
            args.get("status").and_then(|v| v.as_str()).unwrap_or("").trim(),
            args.get("note").and_then(|v| v.as_str()),
        ),
        other => Err(format!("没有这个工具: {other}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 能力查询
// ─────────────────────────────────────────────────────────────────────────

/// 只暴露**已审核通过**且启用的技能。待定池里的东西没经过人看，
/// 不该被别的 agent 当成可信说明书拿去执行。
const VISIBLE_WHERE: &str = "is_active = 1 AND pool = 'official'";

fn search_capabilities(db: &Arc<DbManager>, query: &str) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT name, description, COALESCE(category,''), usage_count
             FROM skills WHERE {VISIBLE_WHERE}"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let terms = tokenize(query);
    let mut scored: Vec<(i64, String, String, String)> = Vec::new();
    for row in rows.flatten() {
        let (name, desc, cat, uses) = row;
        let hay = format!("{name} {desc} {cat}").to_lowercase();
        let hits = terms.iter().filter(|t| hay.contains(*t)).count() as i64;
        if hits == 0 {
            continue;
        }
        // 命中词数为主，用量为辅——常用的技能更可能是对的那个。
        scored.push((hits * 1000 + uses.min(999), name, desc, cat));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(MAX_RESULTS);

    if scored.is_empty() {
        return Ok(format!(
            "没有匹配「{query}」的技能。\
             可以换个说法再搜一次；技能库里只包含已审核通过的能力。"
        ));
    }
    let mut out = format!("找到 {} 个能力（用 {LOAD_TOOL} 取完整说明）：\n\n", scored.len());
    for (_, name, desc, cat) in scored {
        let tag = if cat.is_empty() { String::new() } else { format!("［{cat}］") };
        out.push_str(&format!("- {name}{tag}：{desc}\n"));
    }
    Ok(out)
}

/// 分词：中文没有空格，按空白切完再补一层双字切分，
/// 让「图表」这种词在「做图表」里也能命中。
fn tokenize(q: &str) -> Vec<String> {
    let lower = q.to_lowercase();
    let mut out: Vec<String> = lower
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let cjk: Vec<char> = lower
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .collect();
    for w in cjk.windows(2) {
        out.push(w.iter().collect());
    }
    out.sort();
    out.dedup();
    out
}

fn load_capability(db: &Arc<DbManager>, name: &str) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let path: String = conn
        .query_row(
            &format!(
                "SELECT COALESCE(NULLIF(central_path,''), file_path)
                 FROM skills WHERE name = ?1 AND {VISIBLE_WHERE}"
            ),
            params![name],
            |r| r.get(0),
        )
        .map_err(|_| {
            format!("没有名为「{name}」的可用技能——先用 {SEARCH_TOOL} 查一下确切的名字。")
        })?;

    // P2 技能锁：交出去之前核一遍内容指纹。审核认可的是晋升那一刻的内容，
    // 不是这个文件名——这里是「OMNIX 已审核」这个身份流向别的 agent 前的最后一道检查。
    let status = crate::skill_lock::verify(db, name);
    if !status.is_trusted() {
        return Err(match status {
            crate::skill_lock::LockStatus::Drifted { .. } => format!(
                "技能「{name}」的内容在通过审核之后被改动过，已拒绝提供。\
                 请在 OMNIX 里重新审核并晋升，存证更新后即可正常使用。"
            ),
            crate::skill_lock::LockStatus::Unlocked => format!(
                "技能「{name}」没有内容存证（早于技能锁功能），已拒绝提供。\
                 在 OMNIX 里重新晋升一次即可补上存证。"
            ),
            crate::skill_lock::LockStatus::Missing { reason } => {
                format!("技能「{name}」的文件不可用：{reason}")
            }
            crate::skill_lock::LockStatus::Ok => unreachable!("is_trusted 已排除"),
        });
    }

    let file = std::path::Path::new(&path);
    let file = if file.is_dir() { file.join("SKILL.md") } else { file.to_path_buf() };
    let content = std::fs::read_to_string(&file)
        .map_err(|e| format!("技能「{name}」的文件读不出来（{}）：{e}", file.display()))?;

    // 用量统计：被别的 agent 取走也算用过，技能的优先级才反映真实使用。
    let _ = conn.execute(
        "UPDATE skills SET usage_count = usage_count + 1,
                           last_used_at = CURRENT_TIMESTAMP
         WHERE name = ?1",
        params![name],
    );
    Ok(content)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, params: Value, id: Option<Value>) -> RpcRequest {
        RpcRequest { jsonrpc: "2.0".into(), method: method.into(), params, id }
    }

    fn db() -> Arc<DbManager> {
        let p = std::env::temp_dir()
            .join(format!("omnix_mcp_test_{}_{:?}.db", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_file(&p);
        Arc::new(DbManager::new_with_path(p))
    }

    /// 通知（没有 id）按规范不能回响应。回了会让严格的客户端报协议错。
    #[test]
    fn notifications_get_no_response() {
        let d = db();
        assert!(handle_rpc(&d, req("notifications/initialized", json!({}), None)).is_none());
        assert!(handle_rpc(&d, req("tools/list", json!({}), None)).is_none());
    }

    #[test]
    fn initialize_reports_our_own_version_and_tools() {
        let d = db();
        let r = handle_rpc(&d, req("initialize", json!({}), Some(json!(1)))).unwrap();
        let v = r.result.unwrap();
        assert_eq!(v["protocolVersion"], PROTOCOL_VERSION);
        assert!(v["capabilities"]["tools"].is_object(), "要声明 tools 能力");
        assert_eq!(v["serverInfo"]["name"], "omnix");
    }

    /// 工具数量刻意压到最少：搜 + 取（能力）＋ 产出文件（交付物）。
    /// 每加一个都要能说清它为什么不能并进已有的——工具越多调用方越容易挑错。
    #[test]
    fn tool_surface_stays_minimal_and_fully_specified() {
        let d = db();
        let r = handle_rpc(&d, req("tools/list", json!({}), Some(json!(2)))).unwrap();
        let tools = r.result.unwrap()["tools"].as_array().unwrap().clone();
        // 每个工具都占着对面每一轮的上下文，所以数字写死在这里：多一个就要多一条理由。
        // 现在的 5 个分两组——能力（查/取/出片）与团队协作（看板/上报）。
        assert_eq!(tools.len(), 5, "多一个工具就要多一条理由");
        for t in &tools {
            assert!(t["name"].is_string());
            assert!(!t["description"].as_str().unwrap().is_empty());
            assert_eq!(t["inputSchema"]["type"], "object", "每个工具都要有输入 schema");
            assert!(t["inputSchema"]["required"].is_array());
        }
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for t in [SEARCH_TOOL, LOAD_TOOL, DECK_TOOL, BOARD_TOOL, REPORT_TOOL] {
            assert!(names.contains(&t), "{t} 不在工具清单里");
        }
    }

    /// 工具层面的失败要走 isError，而不是 JSON-RPC error——
    /// 协议层报错会让对面把整个连接当成坏的。
    #[test]
    fn tool_failures_are_reported_as_is_error_not_protocol_errors() {
        let d = db();
        let r = handle_rpc(
            &d,
            req("tools/call", json!({"name": LOAD_TOOL, "arguments": {"name": "不存在"}}), Some(json!(3))),
        )
        .unwrap();
        assert!(r.error.is_none(), "不该是协议层错误");
        let v = r.result.unwrap();
        assert_eq!(v["isError"], true);
        assert!(v["content"][0]["text"].as_str().unwrap().contains(SEARCH_TOOL),
                "错误信息要告诉对面下一步怎么办");
    }

    #[test]
    fn unknown_method_is_a_protocol_error() {
        let d = db();
        let r = handle_rpc(&d, req("tools/nope", json!({}), Some(json!(4)))).unwrap();
        assert_eq!(r.error.unwrap().code, -32601);
    }

    #[test]
    fn empty_arguments_are_rejected_with_a_usable_message() {
        let d = db();
        for (tool, args) in [(SEARCH_TOOL, json!({"query": "  "})), (LOAD_TOOL, json!({"name": ""}))] {
            let r = handle_rpc(
                &d,
                req("tools/call", json!({"name": tool, "arguments": args}), Some(json!(5))),
            )
            .unwrap();
            assert_eq!(r.result.unwrap()["isError"], true, "{tool} 空参数应报错");
        }
    }

    /// 中文查询要能命中——按空白切词对中文是没用的。
    #[test]
    fn tokenizer_handles_chinese_without_spaces() {
        let t = tokenize("把数据做成图表");
        assert!(t.contains(&"图表".to_string()), "{t:?}");
        assert!(t.contains(&"数据".to_string()), "{t:?}");
        // 英文照常按词切
        let e = tokenize("Export to Excel");
        assert!(e.contains(&"excel".to_string()), "{e:?}");
    }

    /// Q3：调用方写内容，OMNIX 出文件。这里**不调模型**——对方本来就是个
    /// 带模型的 agent，OMNIX 只做它做不了的排版和导出。
    #[test]
    fn create_deck_writes_real_files_and_reports_problems() {
        let d = db();
        let r = handle_rpc(&d, req("tools/call", json!({
            "name": DECK_TOOL,
            "arguments": {
                "title": format!("MCP 交付物测试 {}", std::process::id()),
                "slides": [
                    {"layout": "cover", "title": "标题页"},
                    {"layout": "chart", "title": "没有数据的图"}
                ]
            }
        }), Some(json!(9)))).unwrap();
        let v = r.result.unwrap();
        assert_eq!(v["isError"], false, "{v}");
        let text = v["content"][0]["text"].as_str().unwrap();
        assert!(text.contains(".html"), "要返回文件路径: {text}");
        // 体检结果必须一并返回：调用方看不到渲染结果，这是它唯一的反馈渠道
        assert!(text.contains("体检"), "要附体检结果: {text}");
        assert!(text.contains("没有任何数据条目"), "空图表要被点出来: {text}");
    }

    #[test]
    fn create_deck_rejects_empty_input_with_a_usable_message() {
        let d = db();
        for args in [json!({"title": "", "slides": [{}]}), json!({"title": "x", "slides": []})] {
            let r = handle_rpc(&d, req("tools/call",
                json!({"name": DECK_TOOL, "arguments": args}), Some(json!(10)))).unwrap();
            assert_eq!(r.result.unwrap()["isError"], true);
        }
    }

    /// 待审核的技能不能对外暴露：没人看过的说明书不该被别的 agent 当可信的执行。
    #[test]
    fn only_official_skills_are_visible() {
        assert!(VISIBLE_WHERE.contains("pool = 'official'"));
        assert!(VISIBLE_WHERE.contains("is_active = 1"));
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Q3 · 产出交付物
// ─────────────────────────────────────────────────────────────────────────

/// 把调用方写好的幻灯内容渲染成真文件。
///
/// **分工是刻意的**：调用方本来就是个带模型的 agent，让它写内容；OMNIX 只做它
/// 擅长且对方做不了的事——排版、渲染、导出成 .pptx。这里**不调模型**，
/// 所以不花钱、不慢、也不会跟对方的模型抢着写内容。
fn create_deck(args: &Value) -> Result<String, String> {
    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
    if title.is_empty() {
        return Err("title 不能为空。".into());
    }
    let slides = args.get("slides").and_then(|v| v.as_array()).ok_or("slides 必须是数组。")?;
    if slides.is_empty() {
        return Err("slides 至少要有一页。".into());
    }
    let theme = args.get("theme").and_then(|v| v.as_str()).unwrap_or("midnight");

    let mut deck: crate::slides::Deck = serde_json::from_value(serde_json::json!({
        "title": title, "theme": theme, "slides": slides,
    }))
    .map_err(|e| format!("幻灯内容解析失败：{e}"))?;
    for s in deck.slides.iter_mut() {
        s.fill_default_params();
    }

    let dir = crate::storage::exports_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败：{e}"))?;
    let stem: String = title
        .chars()
        .map(|c| if r#"/\:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();

    let html_path = dir.join(format!("{stem}.html"));
    let html = crate::slides::render_deck_html(&deck, None, true);
    let html = crate::slides::embed_deck_source(&html, &deck)?;
    std::fs::write(&html_path, html).map_err(|e| format!("写出 HTML 失败：{e}"))?;

    let pptx_path = dir.join(format!("{stem}.pptx"));
    let pptx_note = match crate::pptx::build_pptx(&deck) {
        Ok(bytes) => match std::fs::write(&pptx_path, bytes) {
            Ok(_) => format!("\n- PowerPoint：{}", pptx_path.display()),
            Err(e) => format!("\n- PowerPoint 写出失败：{e}"),
        },
        Err(e) => format!("\n- PowerPoint 生成失败：{e}"),
    };

    // 体检结果一并返回：调用方看不到渲染结果，这是它唯一能知道
    // 「内容会不会溢出、图表有没有数据」的途径。
    let report = crate::slides_lint::lint_deck(&deck);
    let issues = if report.findings.is_empty() {
        "\n\n体检：没发现问题。".to_string()
    } else {
        let lines: Vec<String> = report
            .findings
            .iter()
            .filter(|f| !matches!(f.severity, crate::slides_lint::Severity::Info))
            .take(8)
            .map(|f| format!("  - {}", f.message))
            .collect();
        if lines.is_empty() {
            "\n\n体检：没发现需要处理的问题。".to_string()
        } else {
            format!("\n\n体检发现 {} 处需要注意：\n{}", lines.len(), lines.join("\n"))
        }
    };

    Ok(format!(
        "已生成 {} 页的《{title}》：\n- 网页：{}{pptx_note}\n\
         （网页文件里嵌了原始数据，可以在 OMNIX 里「导回 HTML」继续编辑）{issues}",
        deck.slides.len(),
        html_path.display(),
    ))
}
