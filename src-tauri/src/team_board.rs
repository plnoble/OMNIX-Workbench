//! T4：团队任务板——让**正在跑的 agent** 看得见并参与协作，而不只是被编排。
//!
//! ## 缺的是哪根线
//!
//! 借鉴 AionUi 的 Team Mode：Leader 通过一个 Team MCP Server 把任务派给
//! Teammate，Teammate 并行执行、把结果写回共享任务板。
//!
//! OMNIX 早就有这套编排的**大半**：`team_generate_plan` 生成带依赖与验收标准
//! 的分工，`team_start_approved_run` 拉起 worker，还有重试、审批、停止。
//! 但计划是**事前**定好由 OMNIX 驱动执行的——一个跑起来的 agent 没有任何
//! 接口能看一眼任务板、报一句进展。协作的信息是单向的。
//!
//! 而 OMNIX 已经在网关上挂了 `/mcp`。两端都在，中间没线——本轮第六次。
//!
//! ## 并发写：这里是 S0 那个坑的原样重现
//!
//! 分工存在 `team_plans.assignments_json` 一个 JSON 大字段里。多个 teammate
//! 同时上报，就是「读整块 → 改一条 → 写整块」的丢更新：后写的把先写的盖掉。
//!
//! S0 那次因为中间隔着几十秒的模型调用，只能用指纹比对。这里的读-改-写是
//! 本地的、瞬时的，所以直接用 **IMMEDIATE 事务**让 SQLite 串行化——
//! 能用事务的时候就别自己造乐观锁。

use std::sync::Arc;

use rusqlite::params;
use serde_json::{json, Value};

use crate::db::DbManager;

/// 允许 agent 上报的状态。**白名单**——agent 报上来的字符串直接写库，
/// 不限死就等于让它往状态机里塞任意值，界面和重试逻辑都会读到看不懂的东西。
const REPORTABLE: &[&str] = &["running", "completed", "failed", "blocked"];

/// 备注截断长度。任务板是给人扫一眼的，不是让 agent 往里灌日志。
const MAX_NOTE: usize = 500;

/// 读任务板。
pub fn board(db: &Arc<DbManager>, run_id: &str) -> Result<String, String> {
    if run_id.trim().is_empty() {
        return Err("run_id 不能为空。".into());
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let (goal, assignments_json, status): (String, String, String) = conn
        .query_row(
            "SELECT goal, assignments_json, status FROM team_plans WHERE run_id = ?1",
            params![run_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| format!("没有 run_id 为「{run_id}」的团队计划。"))?;

    let items: Vec<Value> = serde_json::from_str(&assignments_json).unwrap_or_default();
    if items.is_empty() {
        return Ok(format!("目标：{goal}\n计划状态：{status}\n（这个计划还没有分工）"));
    }
    let mut out = format!("目标：{goal}\n计划状态：{status}\n\n分工：\n");
    for a in &items {
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!(
            "- [{}] {} — {}（负责：{}）",
            s("status"),
            s("id"),
            s("task_title"),
            s("agent_name")
        ));
        // 依赖必须显示：不知道自己卡在谁后面，就无从判断该不该开工。
        if let Some(deps) = a.get("depends_on").and_then(|v| v.as_array()) {
            let names: Vec<&str> = deps.iter().filter_map(|d| d.as_str()).collect();
            if !names.is_empty() {
                out.push_str(&format!("　依赖：{}", names.join("、")));
            }
        }
        if let Some(note) = a.get("note").and_then(|v| v.as_str()) {
            if !note.trim().is_empty() {
                out.push_str(&format!("\n    进展：{note}"));
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// 上报一条分工的状态。
///
/// 整个读-改-写在一个 IMMEDIATE 事务里完成，所以并发上报不会互相覆盖。
pub fn report(
    db: &Arc<DbManager>,
    run_id: &str,
    assignment_id: &str,
    status: &str,
    note: Option<&str>,
) -> Result<String, String> {
    if run_id.trim().is_empty() || assignment_id.trim().is_empty() {
        return Err("run_id 和 assignment_id 都不能为空。".into());
    }
    if !REPORTABLE.contains(&status) {
        return Err(format!(
            "status 只能是 {}——报别的值界面和重试逻辑都读不懂。",
            REPORTABLE.join(" / ")
        ));
    }
    let mut conn = db.get_connection().map_err(|e| e.to_string())?;
    // IMMEDIATE：一开始就拿写锁，避免两个 teammate 都读完了才发现要写。
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;

    let assignments_json: String = tx
        .query_row(
            "SELECT assignments_json FROM team_plans WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("没有 run_id 为「{run_id}」的团队计划。"))?;

    let mut items: Vec<Value> =
        serde_json::from_str(&assignments_json).map_err(|e| format!("任务板已损坏：{e}"))?;
    let target = items
        .iter_mut()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(assignment_id))
        .ok_or_else(|| {
            format!("这个计划里没有 id 为「{assignment_id}」的分工——先用 team_board 看一眼。")
        })?;

    let Some(obj) = target.as_object_mut() else {
        return Err("任务板条目格式异常。".into());
    };
    obj.insert("status".into(), json!(status));
    if let Some(n) = note {
        let n = n.trim();
        if !n.is_empty() {
            obj.insert("note".into(), json!(n.chars().take(MAX_NOTE).collect::<String>()));
        }
    }

    let updated = serde_json::to_string(&items).map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE team_plans SET assignments_json = ?1 WHERE run_id = ?2",
        params![updated, run_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;

    Ok(format!("已记录：{assignment_id} → {status}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Arc<DbManager> {
        let path = std::env::temp_dir().join(format!(
            "omnix_board_{}_{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let _ = std::fs::remove_file(&path);
        let d = Arc::new(DbManager::new_runtime_test(path));
        d.get_connection()
            .unwrap()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS team_plans (
                    run_id TEXT PRIMARY KEY, goal TEXT NOT NULL,
                    assignments_json TEXT NOT NULL DEFAULT '[]',
                    status TEXT NOT NULL DEFAULT 'draft',
                    created_at TEXT DEFAULT (datetime('now')), approved_at TEXT);",
            )
            .unwrap();
        d
    }

    fn seed(d: &Arc<DbManager>) {
        d.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO team_plans (run_id, goal, assignments_json, status) VALUES ('r1', '把网关修好', ?1, 'approved')",
                params![json!([
                    {"id": "a1", "agent_name": "Claude Code", "task_title": "改网关", "status": "pending", "depends_on": []},
                    {"id": "a2", "agent_name": "Codex", "task_title": "补测试", "status": "pending", "depends_on": ["a1"]}
                ]).to_string()],
            )
            .unwrap();
    }

    #[test]
    fn board_shows_who_owns_what_and_what_blocks_it() {
        let d = db();
        seed(&d);
        let text = board(&d, "r1").unwrap();
        assert!(text.contains("把网关修好"));
        assert!(text.contains("a1"), "要给出可上报的 id：{text}");
        assert!(text.contains("Claude Code"));
        // 不知道自己卡在谁后面就无从判断该不该开工。
        assert!(text.contains("依赖：a1"), "依赖必须显示：{text}");
    }

    #[test]
    fn reporting_updates_only_the_named_assignment() {
        let d = db();
        seed(&d);
        report(&d, "r1", "a1", "completed", Some("网关已修，测试通过")).unwrap();
        let text = board(&d, "r1").unwrap();
        assert!(text.contains("[completed] a1"));
        assert!(text.contains("网关已修"));
        assert!(text.contains("[pending] a2"), "别的分工不该被动到：{text}");
    }

    #[test]
    fn concurrent_reports_do_not_overwrite_each_other() {
        // 分工存在一个 JSON 大字段里，读-改-写天然会丢更新——这正是 S0 那个坑。
        // 两个 teammate 各报各的，两条都必须留下。
        let d = db();
        seed(&d);
        let (d1, d2) = (Arc::clone(&d), Arc::clone(&d));
        let h1 = std::thread::spawn(move || report(&d1, "r1", "a1", "completed", None));
        let h2 = std::thread::spawn(move || report(&d2, "r1", "a2", "failed", None));
        h1.join().unwrap().unwrap();
        h2.join().unwrap().unwrap();

        let text = board(&d, "r1").unwrap();
        assert!(text.contains("[completed] a1"), "a1 的上报被盖掉了：{text}");
        assert!(text.contains("[failed] a2"), "a2 的上报被盖掉了：{text}");
    }

    #[test]
    fn arbitrary_status_strings_are_rejected() {
        // agent 报上来的字符串直接写库，不限死就等于让它往状态机里塞任意值。
        let d = db();
        seed(&d);
        let e = report(&d, "r1", "a1", "差不多完成了", None).unwrap_err();
        assert!(e.contains("completed"), "错误要告诉它合法值是什么：{e}");
        // 库里必须没被改。
        assert!(board(&d, "r1").unwrap().contains("[pending] a1"));
    }

    #[test]
    fn unknown_ids_say_what_to_do_next() {
        let d = db();
        seed(&d);
        assert!(report(&d, "r1", "a9", "completed", None).unwrap_err().contains("team_board"));
        assert!(board(&d, "不存在").unwrap_err().contains("不存在"));
    }

    #[test]
    fn a_flood_of_notes_cannot_turn_the_board_into_a_log() {
        let d = db();
        seed(&d);
        let flood = "秘".repeat(MAX_NOTE * 4);
        report(&d, "r1", "a1", "running", Some(&flood)).unwrap();
        let text = board(&d, "r1").unwrap();
        // 用一个夹具里不会出现的字符来数——第一版用 'x'，结果把队友名
        // "Codex" 里的那个也数进去了，断言少了一位。
        let n = text.chars().filter(|c| *c == '秘').count();
        assert_eq!(n, MAX_NOTE, "备注必须被截断到 {MAX_NOTE} 字，实际 {n}");
    }
}
