use super::*;
use crate::agent::{parse_schedule, run_cron_task, SCHEDULE_HELP};
use crate::db::DbManager;
use crate::input_validation;
use rusqlite::params;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_cron_tasks(db: State<'_, Arc<DbManager>>) -> Result<Vec<CronTask>, String> {
    get_cron_tasks_core(&db)
}

pub(crate) fn get_cron_tasks_core(db: &DbManager) -> Result<Vec<CronTask>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, title, schedule, agent_name, args, workspace_dir, is_active, last_run, created_at
         FROM cron_tasks ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let is_active_int: i32 = row.get(6)?;
            Ok(CronTask {
                id: row.get(0)?,
                title: row.get(1)?,
                schedule: row.get(2)?,
                agent_name: row.get(3)?,
                args: row.get(4)?,
                workspace_dir: row.get(5)?,
                is_active: is_active_int != 0,
                last_run: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(task) = r {
            result.push(task);
        }
    }
    Ok(result)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_cron_task(
    id: String,
    title: String,
    schedule: String,
    agent_name: String,
    args: String,
    workspace_dir: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    save_cron_task_core(
        &db,
        &id,
        &title,
        &schedule,
        &agent_name,
        &args,
        &workspace_dir,
        is_active,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn save_cron_task_core(
    db: &DbManager,
    id: &str,
    title: &str,
    schedule: &str,
    agent_name: &str,
    args: &str,
    workspace_dir: &str,
    is_active: bool,
) -> Result<(), String> {
    input_validation::validate_id(id, "id")?;
    input_validation::validate_name(agent_name, "agent_name")?;
    // 表达式**必须在这里认一遍**。调度器碰到认不出来的就当「不该跑」，所以一条
    // 拼错的表达式过去是存得进去的：列表里显示「已启用」，然后永远不触发、也
    // 不报错。用的是调度器那同一份解析，不会两边漂开。
    if parse_schedule(schedule).is_none() {
        return Err(format!("看不懂这个调度表达式「{schedule}」。{SCHEDULE_HELP}"));
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO cron_tasks (id, title, schedule, agent_name, args, workspace_dir, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            schedule = excluded.schedule,
            agent_name = excluded.agent_name,
            args = excluded.args,
            workspace_dir = excluded.workspace_dir,
            is_active = excluded.is_active",
        params![
            id,
            title,
            schedule,
            agent_name,
            args,
            workspace_dir,
            if is_active { 1 } else { 0 }
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_cron_task_active(
    id: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    toggle_cron_task_active_core(&db, &id, is_active)
}

pub(crate) fn toggle_cron_task_active_core(
    db: &DbManager,
    id: &str,
    is_active: bool,
) -> Result<(), String> {
    input_validation::validate_id(id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE cron_tasks SET is_active = ?1 WHERE id = ?2",
        params![if is_active { 1 } else { 0 }, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_cron_task(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    delete_cron_task_core(&db, &id)
}

pub(crate) fn delete_cron_task_core(db: &DbManager, id: &str) -> Result<(), String> {
    input_validation::validate_id(id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    // 运行记录得跟着走。`cron_runs.task_id` 上没有外键，删任务不清它的话那些行
    // 会一直留着：「最近 50 条运行」里于是混着已经不存在的任务，而且一条卡在
    // `running` 的孤儿记录还会挡住同 id 任务的下一次触发（防叠加那条查的就是它）。
    conn.execute("DELETE FROM cron_runs WHERE task_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM cron_tasks WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_cron_runs(db: State<'_, Arc<DbManager>>) -> Result<Vec<CronRun>, String> {
    get_cron_runs_core(&db)
}

pub(crate) fn get_cron_runs_core(db: &DbManager) -> Result<Vec<CronRun>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, task_id, status, log_path, started_at, finished_at
         FROM cron_runs ORDER BY started_at DESC LIMIT 50",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CronRun {
                id: row.get(0)?,
                task_id: row.get(1)?,
                status: row.get(2)?,
                log_path: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(run) = r {
            result.push(run);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn clear_cron_runs(db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM cron_runs", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn trigger_cron_task(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, agent_name, args, workspace_dir FROM cron_tasks WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    let row = stmt
        .query_row(params![id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("Cron task not found: {}", e))?;

    let (task_id, agent_name, args_str, workspace_dir) = row;
    let db_arc = db.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = run_cron_task(db_arc, task_id, agent_name, args_str, workspace_dir).await;
    });
    Ok(())
}

// ── MCP Servers ──────────────────────────────────────────

/// MCP Server DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: String,
    pub env: String,
    pub url: String,
    pub server_type: String,
    pub is_enabled: bool,
}

/// Get all configured MCP servers.
#[tauri::command]
pub fn get_mcp_servers(db: State<'_, Arc<DbManager>>) -> Result<Vec<McpServer>, String> {
    let rows = db.get_mcp_servers().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(id, name, command, args, env, url, server_type, is_enabled)| McpServer {
                id,
                name,
                command,
                args,
                env,
                url,
                server_type,
                is_enabled,
            },
        )
        .collect())
}

/// Save (upsert) an MCP server configuration.
#[tauri::command]
pub fn save_mcp_server(server: McpServer, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    db.save_mcp_server(
        &server.id,
        &server.name,
        &server.command,
        &server.args,
        &server.env,
        &server.url,
        &server.server_type,
        server.is_enabled,
    )
    .map_err(|e| e.to_string())
}

/// Delete an MCP server.
#[tauri::command]
pub fn delete_mcp_server(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    db.delete_mcp_server(&id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_cron_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    fn save(db: &DbManager, id: &str, title: &str, schedule: &str) -> Result<(), String> {
        save_cron_task_core(db, id, title, schedule, "Claude Code", "{}", "/tmp", true)
    }

    fn seed_run(db: &DbManager, run_id: &str, task_id: &str, started_at: &str) {
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO cron_runs (id, task_id, status, log_path, started_at)
                 VALUES (?1, ?2, 'running', '', ?3)",
                params![run_id, task_id, started_at],
            )
            .unwrap();
    }

    /// 认不出来的表达式**不能存进去**。
    ///
    /// 调度器对认不出来的一律当「不该跑」，所以这种任务过去存得下、列表里显示
    /// 「已启用」、然后永远不触发也不报错。区间和列表（`0 9 * * 1-5`）是最容易
    /// 中招的一种——它是标准 cron 写法，但这里的匹配器不支持。
    #[test]
    fn an_unrecognized_schedule_is_refused_at_save_time() {
        let db = test_db("badsched");
        for bad in [
            "0 9 * * 1-5", // 标准 cron 的区间，这里不支持
            "0 9 * * 1,3", // 列表，同理
            "every 5 min",  // 少了 "utes"
            "daily at 25:00",
            "*/0 * * * *", // 取模会除零
            "随便写点什么",
            "",
        ] {
            let result = save(&db, "t1", "任务", bad);
            assert!(result.is_err(), "「{bad}」不该被存下来");
            assert!(
                result.unwrap_err().contains("支持"),
                "报错要告诉用户支持什么写法"
            );
        }
        assert!(get_cron_tasks_core(&db).unwrap().is_empty());
    }

    /// 真正认得的几种写法一个都不能被误伤。
    #[test]
    fn every_supported_schedule_form_still_saves() {
        let db = test_db("goodsched");
        for (i, good) in [
            "*/15 * * * *",
            "0 9 * * 1",
            "* * * * *",
            "every 30 minutes",
            "every 2 hours",
            "daily at 09:30",
            "Daily At 23:59", // 大小写不敏感
        ]
        .iter()
        .enumerate()
        {
            save(&db, &format!("t{i}"), "任务", good).unwrap_or_else(|e| panic!("{good}: {e}"));
        }
        assert_eq!(get_cron_tasks_core(&db).unwrap().len(), 7);
    }

    /// 改一条已存在的任务，不能把它的 `last_run` 抹掉。
    ///
    /// `last_run` 是「上次跑到哪了」的唯一凭据：被清成 NULL 的话，`every N
    /// minutes` 这类会立刻判定为该跑，改个标题就触发一次计划外执行。
    #[test]
    fn editing_a_task_keeps_its_last_run() {
        let db = test_db("upsert");
        save(&db, "t1", "原标题", "every 30 minutes").unwrap();
        db.get_connection()
            .unwrap()
            .execute(
                "UPDATE cron_tasks SET last_run = '2026-08-01 10:00:00' WHERE id = 't1'",
                [],
            )
            .unwrap();

        save(&db, "t1", "新标题", "every 30 minutes").unwrap();

        let tasks = get_cron_tasks_core(&db).unwrap();
        assert_eq!(tasks.len(), 1, "upsert 变成了插入第二行");
        assert_eq!(tasks[0].title, "新标题");
        assert_eq!(
            tasks[0].last_run.as_deref(),
            Some("2026-08-01 10:00:00"),
            "保存把 last_run 清掉了"
        );
    }

    /// 删任务要把它的运行记录一起带走。
    ///
    /// `cron_runs.task_id` 没有外键，级联救不了。留下来的孤儿记录会占满「最近
    /// 50 条」，而卡在 `running` 的那条还会挡住同 id 任务的下一次触发。
    #[test]
    fn deleting_a_task_takes_its_runs_with_it() {
        let db = test_db("delete");
        save(&db, "t1", "要删的", "every 30 minutes").unwrap();
        save(&db, "t2", "留下的", "every 30 minutes").unwrap();
        seed_run(&db, "r1", "t1", "2026-08-01 10:00:00");
        seed_run(&db, "r2", "t2", "2026-08-01 11:00:00");

        delete_cron_task_core(&db, "t1").unwrap();

        let ids: Vec<String> = get_cron_tasks_core(&db)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec!["t2".to_string()]);
        let runs: Vec<String> = get_cron_runs_core(&db)
            .unwrap()
            .into_iter()
            .map(|r| r.task_id)
            .collect();
        assert_eq!(runs, vec!["t2".to_string()], "被删任务的运行记录还留着");
    }

    /// 运行列表按开始时间倒序，最多 50 条。
    #[test]
    fn runs_are_newest_first_and_capped() {
        let db = test_db("runs");
        save(&db, "t1", "任务", "every 30 minutes").unwrap();
        for i in 0..55 {
            seed_run(
                &db,
                &format!("r{i:03}"),
                "t1",
                &format!("2026-08-01 {:02}:{:02}:00", i / 60, i % 60),
            );
        }
        let runs = get_cron_runs_core(&db).unwrap();
        assert_eq!(runs.len(), 50, "没有截到 50 条");
        assert_eq!(runs[0].id, "r054", "不是最新的在最前面");
        assert_eq!(runs[49].id, "r005");
    }

    /// 开关只动目标那一行。
    #[test]
    fn toggling_one_task_leaves_the_others_alone() {
        let db = test_db("toggle");
        save(&db, "t1", "一", "every 30 minutes").unwrap();
        save(&db, "t2", "二", "every 30 minutes").unwrap();

        toggle_cron_task_active_core(&db, "t1", false).unwrap();

        let tasks = get_cron_tasks_core(&db).unwrap();
        let t1 = tasks.iter().find(|t| t.id == "t1").unwrap();
        let t2 = tasks.iter().find(|t| t.id == "t2").unwrap();
        assert!(!t1.is_active);
        assert!(t2.is_active, "误改了别的任务");
    }

    /// 空 id / 空 agent 名不能一路走到 SQL。
    #[test]
    fn blank_ids_are_rejected_before_touching_sql() {
        let db = test_db("validate");
        assert!(save(&db, "  ", "任务", "every 30 minutes").is_err());
        assert!(save_cron_task_core(
            &db,
            "t1",
            "任务",
            "every 30 minutes",
            "  ",
            "{}",
            "/tmp",
            true
        )
        .is_err());
        assert!(toggle_cron_task_active_core(&db, "", true).is_err());
        assert!(delete_cron_task_core(&db, "").is_err());
    }
}
