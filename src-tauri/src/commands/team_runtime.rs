use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::task::JoinSet;

use crate::agent::AgentManager;
use crate::db::DbManager;
use crate::runtime::{
    AgentId, AgentSessionConfig, AgentSessionStatus, ModelSelection, PermissionPolicy,
    RuntimeEventKind, WorkMode,
};
use crate::runtime_manager::RuntimeManager;

use super::runs::{
    create_workspace_run_core, get_team_plan_core, get_workspace_run_core, list_agent_runs_core,
    AgentRun, TeamAssignment, TeamPlan, WorkspaceRun,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRunDetail {
    pub run: WorkspaceRun,
    pub plan: Option<TeamPlan>,
    pub workers: Vec<AgentRun>,
}

#[derive(Debug, Deserialize)]
struct GeneratedPlan {
    assignments: Vec<GeneratedAssignment>,
}

#[derive(Debug, Deserialize)]
struct GeneratedAssignment {
    id: String,
    agent_name: String,
    task_title: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    #[serde(default = "default_max_retries")]
    max_retries: i64,
}

fn default_max_retries() -> i64 {
    1
}

fn parse_agent(name: &str) -> Result<AgentId, String> {
    match name.trim().to_lowercase().as_str() {
        "claude code" | "claude_code" | "claude" => Ok(AgentId::ClaudeCode),
        "codex" | "codex cli" => Ok(AgentId::Codex),
        _ => Err(format!("Team 首期只支持 Claude Code 与 Codex：{name}")),
    }
}

fn clean_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    let without_prefix = trimmed.strip_prefix("```json").unwrap_or(trimmed);
    without_prefix.trim_end_matches("```").trim()
}

fn validate_assignments(assignments: &[TeamAssignment]) -> Result<(), String> {
    if assignments.is_empty() {
        return Err("队长没有生成 Worker 任务".into());
    }
    let ids = assignments
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    if ids.len() != assignments.len() || ids.contains("") {
        return Err("队长计划包含空 ID 或重复 ID".into());
    }
    for assignment in assignments {
        parse_agent(&assignment.agent_name)?;
        if assignment.task_title.trim().is_empty() {
            return Err(format!("任务 {} 缺少任务说明", assignment.id));
        }
        if assignment.max_retries < 0 || assignment.max_retries > 3 {
            return Err(format!(
                "任务 {} 的重试次数必须在 0 到 3 之间",
                assignment.id
            ));
        }
        for dependency in &assignment.depends_on {
            if dependency == &assignment.id || !ids.contains(dependency.as_str()) {
                return Err(format!("任务 {} 的依赖 {} 无效", assignment.id, dependency));
            }
        }
    }

    fn visit<'a>(
        id: &'a str,
        map: &HashMap<&'a str, &'a TeamAssignment>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
    ) -> Result<(), String> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(format!("队长计划存在循环依赖：{id}"));
        }
        if let Some(assignment) = map.get(id) {
            for dependency in &assignment.depends_on {
                visit(dependency, map, visiting, visited)?;
            }
        }
        visiting.remove(id);
        visited.insert(id);
        Ok(())
    }

    let map = assignments
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in ids {
        visit(id, &map, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn parse_manager_plan(raw: &str) -> Result<Vec<TeamAssignment>, String> {
    let generated: GeneratedPlan = serde_json::from_str(clean_json(raw))
        .map_err(|error| format!("队长返回的计划 JSON 无法解析：{error}"))?;
    let assignments = generated
        .assignments
        .into_iter()
        .map(|item| TeamAssignment {
            id: item.id.trim().to_string(),
            agent_name: item.agent_name.trim().to_string(),
            task_title: item.task_title.trim().to_string(),
            status: "planned".into(),
            depends_on: item.depends_on,
            acceptance_criteria: item.acceptance_criteria,
            max_retries: item.max_retries,
        })
        .collect::<Vec<_>>();
    validate_assignments(&assignments)?;
    Ok(assignments)
}

fn get_detail(db: &Arc<DbManager>, run_id: &str) -> Result<TeamRunDetail, String> {
    let run = get_workspace_run_core(db, run_id)?;
    let plan = get_team_plan_core(db, run_id).ok();
    let workers = list_agent_runs_core(db, run_id)?;
    Ok(TeamRunDetail { run, plan, workers })
}

fn create_runtime_conversation(
    db: &DbManager,
    conversation_id: &str,
    title: &str,
    workspace_path: &str,
    agent: AgentId,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO conversations (id, title, workspace_path, active_agent)
         VALUES (?1, ?2, ?3, ?4)",
        params![conversation_id, title, workspace_path, agent.display_name()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn execute_turn(
    runtime: Arc<RuntimeManager>,
    agent_manager: Arc<AgentManager>,
    db: Arc<DbManager>,
    conversation_id: String,
    workspace_path: String,
    agent: AgentId,
    mode: WorkMode,
    prompt: String,
    timeout: Duration,
    worker_id: Option<String>,
) -> Result<(String, String), String> {
    let executable_path = agent_manager
        .find_agent_path(agent.display_name())
        .ok_or_else(|| format!("{} 未安装或无法检测", agent.display_name()))?;
    create_runtime_conversation(
        &db,
        &conversation_id,
        &prompt.chars().take(48).collect::<String>(),
        &workspace_path,
        agent,
    )?;
    let mut receiver = runtime.subscribe();
    let session = runtime
        .start_session(AgentSessionConfig {
            conversation_id,
            agent,
            executable_path,
            workspace_path,
            model: ModelSelection::AgentDefault,
            permission: PermissionPolicy::AskOnRisk,
            work_mode: mode,
        })
        .await?;
    let session_id = session.id.clone();
    if let Some(worker_id) = worker_id.as_deref() {
        update_worker(&db, worker_id, "running", Some(&session_id), None, false)?;
    }
    runtime
        .send_message_with_display(&session_id, &prompt, &prompt)
        .await?;
    let future = async {
        let mut answer = String::new();
        loop {
            tokio::select! {
                envelope = receiver.recv() => {
                    let envelope = envelope.map_err(|error| error.to_string())?;
                    if envelope.session_id != session_id { continue; }
                    match envelope.event.kind {
                        RuntimeEventKind::AssistantMessage | RuntimeEventKind::Plan => {
                            if let Some(text) = envelope.event.text.filter(|text| !text.trim().is_empty()) {
                                answer = text;
                            }
                        }
                        RuntimeEventKind::Error => {
                            return Err(envelope.event.text.unwrap_or_else(|| "Agent 运行失败".into()));
                        }
                        RuntimeEventKind::ApprovalRequested => {
                            if let Some(worker_id) = worker_id.as_deref() {
                                update_worker(&db, worker_id, "awaiting_approval", Some(&session_id), None, false)?;
                            }
                        }
                        RuntimeEventKind::TurnCompleted => {
                            if answer.trim().is_empty() {
                                return Err("Agent 完成了任务，但没有返回可用结果".into());
                            }
                            return Ok(answer);
                        }
                        _ => {}
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    match runtime.get_session(&session_id)?.status {
                        AgentSessionStatus::Cancelled => return Err("Agent 会话已取消".into()),
                        AgentSessionStatus::Failed => return Err("Agent 会话运行失败".into()),
                        _ => {}
                    }
                }
            }
        }
    };
    let result = match tokio::time::timeout(timeout, future).await {
        Ok(result) => result,
        Err(_) => Err("Agent 单轮运行超时".to_string()),
    };
    if result.is_ok() {
        let _ = runtime.complete_session(&session_id).await;
    } else {
        let _ = runtime.stop_session(&session_id).await;
    }
    result.map(|answer| (session_id, answer))
}

fn store_plan(
    db: &Arc<DbManager>,
    run_id: &str,
    goal: &str,
    assignments: &[TeamAssignment],
) -> Result<(), String> {
    let assignments_json = serde_json::to_string(assignments).map_err(|error| error.to_string())?;
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO team_plans (run_id, goal, assignments_json, status, approved_at)
         VALUES (?1, ?2, ?3, 'proposed', NULL)",
        params![run_id, goal, assignments_json],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE workspace_runs SET status = 'awaiting_plan_approval', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn team_generate_plan(
    goal: String,
    workspace_path: String,
    manager_agent: String,
    db: State<'_, Arc<DbManager>>,
    runtime: State<'_, Arc<RuntimeManager>>,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<TeamRunDetail, String> {
    if goal.trim().is_empty() {
        return Err("请先填写团队目标".into());
    }
    let manager = parse_agent(&manager_agent)?;
    let run = create_workspace_run_core(
        &db,
        &goal.chars().take(60).collect::<String>(),
        &workspace_path,
        manager.display_name(),
    )?;
    let prompt = format!(
        r#"你是 OMNIX Workbench 的 Team 队长。只生成计划，不执行任务。
将目标拆给已支持的 Worker：Claude Code、Codex。返回严格 JSON，不要 Markdown：
{{"assignments":[{{"id":"task_1","agent_name":"Claude Code|Codex","task_title":"具体任务与边界","depends_on":[],"acceptance_criteria":["可验证标准"],"max_retries":1}}]}}
要求：任务可独立验收；依赖使用 assignment id；避免多人修改同一文件；至少一个任务，最多八个任务。

团队目标：
{}"#,
        goal
    );
    let conversation_id = format!("team_manager_{}_{}", run.id, Utc::now().timestamp_micros());
    let result = execute_turn(
        Arc::clone(&runtime),
        Arc::clone(&agent_manager),
        Arc::clone(&db),
        conversation_id,
        workspace_path,
        manager,
        WorkMode::Plan,
        prompt,
        Duration::from_secs(300),
        None,
    )
    .await;
    let (_, raw_plan) = match result {
        Ok(value) => value,
        Err(error) => {
            let conn = db
                .get_connection()
                .map_err(|db_error| db_error.to_string())?;
            let _ = conn.execute(
                "UPDATE workspace_runs SET status = 'failed', summary = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![error, run.id],
            );
            return Err(error);
        }
    };
    let assignments = parse_manager_plan(&raw_plan)?;
    if assignments.len() > 8 {
        return Err("队长计划超过八个 Worker 任务，请缩小目标".into());
    }
    store_plan(&db, &run.id, &goal, &assignments)?;
    get_detail(&db, &run.id)
}

#[tauri::command]
pub fn team_get_run_detail(
    run_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<TeamRunDetail, String> {
    get_detail(&db, &run_id)
}

fn seed_worker_runs(db: &Arc<DbManager>, run_id: &str, plan: &TeamPlan) -> Result<(), String> {
    validate_assignments(&plan.assignments)?;
    let mut conn = db.get_connection().map_err(|error| error.to_string())?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    for assignment in &plan.assignments {
        let id = format!("worker_{}_{}", run_id, assignment.id);
        transaction
            .execute(
                "INSERT OR IGNORE INTO agent_runs
             (id, run_id, agent_name, task_title, status, assignment_id, dependencies_json,
              acceptance_json, max_retries, validation_status)
             VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6, ?7, ?8, 'pending')",
                params![
                    id,
                    run_id,
                    assignment.agent_name,
                    assignment.task_title,
                    assignment.id,
                    serde_json::to_string(&assignment.depends_on).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&assignment.acceptance_criteria)
                        .unwrap_or_else(|_| "[]".into()),
                    assignment.max_retries,
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.execute(
        "UPDATE workspace_runs SET status = 'running', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    ).map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

fn update_worker(
    db: &DbManager,
    worker_id: &str,
    status: &str,
    session_id: Option<&str>,
    result: Option<&str>,
    increment_retry: bool,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE agent_runs SET status = ?1,
            session_id = COALESCE(?2, session_id),
            result_summary = COALESCE(?3, result_summary),
            log_excerpt = CASE WHEN ?3 IS NULL THEN log_excerpt ELSE substr(?3, 1, 1200) END,
            retry_count = retry_count + ?4,
            started_at = CASE WHEN ?1 = 'running' THEN COALESCE(started_at, CURRENT_TIMESTAMP) ELSE started_at END,
            completed_at = CASE WHEN ?1 IN ('completed','failed','cancelled','blocked') THEN CURRENT_TIMESTAMP ELSE completed_at END
         WHERE id = ?5",
        params![status, session_id, result, if increment_retry { 1 } else { 0 }, worker_id],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

async fn run_worker(
    db: Arc<DbManager>,
    runtime: Arc<RuntimeManager>,
    agent_manager: Arc<AgentManager>,
    run: WorkspaceRun,
    plan: TeamPlan,
    worker: AgentRun,
) -> Result<(), String> {
    let agent = parse_agent(&worker.agent_name)?;
    let dependency_results = list_agent_runs_core(&db, &run.id)?
        .into_iter()
        .filter(|candidate| worker.dependencies.contains(&candidate.assignment_id))
        .map(|candidate| format!("{}: {}", candidate.assignment_id, candidate.result_summary))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "你是 Team Worker。只负责当前分工，并在完成后说明修改、验证命令和结果。\n团队目标：{}\n当前任务：{}\n验收标准：{}\n依赖结果：{}",
        plan.goal,
        worker.task_title,
        worker.acceptance_criteria.join("；"),
        if dependency_results.is_empty() { "无" } else { &dependency_results },
    );
    let max_attempts = worker.max_retries + 1;
    for attempt in 0..max_attempts {
        update_worker(&db, &worker.id, "running", None, None, attempt > 0)?;
        let conversation_id = format!(
            "team_worker_{}_{}_{}",
            run.id, worker.assignment_id, attempt
        );
        match execute_turn(
            Arc::clone(&runtime),
            Arc::clone(&agent_manager),
            Arc::clone(&db),
            conversation_id,
            run.workspace_path.clone(),
            agent,
            WorkMode::Direct,
            prompt.clone(),
            Duration::from_secs(1800),
            Some(worker.id.clone()),
        )
        .await
        {
            Ok((session_id, result)) => {
                update_worker(
                    &db,
                    &worker.id,
                    "completed",
                    Some(&session_id),
                    Some(&result),
                    false,
                )?;
                return Ok(());
            }
            Err(error) => {
                if get_workspace_run_core(&db, &run.id)?.status == "cancelled" {
                    update_worker(&db, &worker.id, "cancelled", None, Some(&error), false)?;
                    return Ok(());
                }
                if attempt + 1 < max_attempts {
                    update_worker(&db, &worker.id, "retrying", None, Some(&error), false)?;
                    continue;
                }
                update_worker(&db, &worker.id, "failed", None, Some(&error), false)?;
                return Err(error);
            }
        }
    }
    Err("Worker exhausted retry budget".into())
}

async fn validate_team_result(
    db: Arc<DbManager>,
    runtime: Arc<RuntimeManager>,
    agent_manager: Arc<AgentManager>,
    run: &WorkspaceRun,
    plan: &TeamPlan,
) -> Result<String, String> {
    let workers = list_agent_runs_core(&db, &run.id)?;
    let evidence = workers
        .iter()
        .map(|worker| {
            format!(
                "- {} / {}\n  验收：{}\n  结果：{}",
                worker.assignment_id,
                worker.task_title,
                worker.acceptance_criteria.join("；"),
                worker.result_summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "你是 Team 队长的最终验收角色。检查工作区实际状态，并根据目标与 Worker 证据给出简洁验收结论。必须明确写出 PASS 或 FAIL。\n目标：{}\nWorker 结果：\n{}",
        plan.goal, evidence
    );
    let manager = parse_agent(&run.manager_agent)?;
    let conversation_id = format!(
        "team_validation_{}_{}",
        run.id,
        Utc::now().timestamp_micros()
    );
    execute_turn(
        runtime,
        agent_manager,
        db,
        conversation_id,
        run.workspace_path.clone(),
        manager,
        WorkMode::Plan,
        prompt,
        Duration::from_secs(600),
        None,
    )
    .await
    .map(|(_, answer)| answer)
}

async fn run_scheduler(
    db: Arc<DbManager>,
    runtime: Arc<RuntimeManager>,
    agent_manager: Arc<AgentManager>,
    run_id: String,
    concurrency: usize,
) {
    let result: Result<(), String> = async {
        loop {
            let run = get_workspace_run_core(&db, &run_id)?;
            if run.status == "cancelled" { return Ok(()); }
            let plan = get_team_plan_core(&db, &run_id)?;
            let workers = list_agent_runs_core(&db, &run_id)?;
            let completed = workers.iter().filter(|worker| worker.status == "completed")
                .map(|worker| worker.assignment_id.clone()).collect::<HashSet<_>>();
            if workers.iter().all(|worker| worker.status == "completed") {
                let validation = validate_team_result(
                    Arc::clone(&db), Arc::clone(&runtime), Arc::clone(&agent_manager), &run, &plan,
                ).await?;
                let passed = validation.to_uppercase().contains("PASS") && !validation.to_uppercase().contains("FAIL");
                let conn = db.get_connection().map_err(|error| error.to_string())?;
                conn.execute(
                    "UPDATE workspace_runs SET status = ?1, summary = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
                    params![if passed { "completed" } else { "validation_failed" }, validation, run_id],
                ).map_err(|error| error.to_string())?;
                conn.execute(
                    "UPDATE agent_runs SET validation_status = ?1 WHERE run_id = ?2 AND status = 'completed'",
                    params![if passed { "passed" } else { "failed" }, run_id],
                ).map_err(|error| error.to_string())?;
                return Ok(());
            }
            if workers.iter().any(|worker| worker.status == "failed") {
                let failed_ids = workers.iter().filter(|worker| worker.status == "failed")
                    .map(|worker| worker.assignment_id.clone()).collect::<HashSet<_>>();
                let conn = db.get_connection().map_err(|error| error.to_string())?;
                for worker in &workers {
                    if worker.status == "queued" && worker.dependencies.iter().any(|dep| failed_ids.contains(dep)) {
                        let _ = conn.execute(
                            "UPDATE agent_runs SET status = 'blocked', completed_at = CURRENT_TIMESTAMP WHERE id = ?1",
                            params![worker.id],
                        );
                    }
                }
                conn.execute(
                    "UPDATE workspace_runs SET status = 'failed', summary = '一个或多个 Worker 失败', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                    params![run_id],
                ).map_err(|error| error.to_string())?;
                return Ok(());
            }
            let ready = workers.into_iter()
                .filter(|worker| matches!(worker.status.as_str(), "queued" | "retrying"))
                .filter(|worker| worker.dependencies.iter().all(|dependency| completed.contains(dependency)))
                .take(concurrency.max(1))
                .collect::<Vec<_>>();
            if ready.is_empty() {
                return Err("没有可运行的 Worker，计划可能处于阻塞状态".into());
            }
            let mut set = JoinSet::new();
            for worker in ready {
                set.spawn(run_worker(
                    Arc::clone(&db), Arc::clone(&runtime), Arc::clone(&agent_manager),
                    run.clone(), plan.clone(), worker,
                ));
            }
            while let Some(joined) = set.join_next().await {
                if let Err(error) = joined { log::error!("Team Worker task join failed: {error}"); }
            }
        }
    }.await;
    if let Err(error) = result {
        if let Ok(conn) = db.get_connection() {
            let _ = conn.execute(
                "UPDATE workspace_runs SET status = 'failed', summary = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2 AND status <> 'cancelled'",
                params![error, run_id],
            );
        }
    }
}

#[tauri::command]
pub async fn team_start_approved_run(
    run_id: String,
    concurrency: Option<usize>,
    db: State<'_, Arc<DbManager>>,
    runtime: State<'_, Arc<RuntimeManager>>,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<TeamRunDetail, String> {
    let plan = get_team_plan_core(&db, &run_id)?;
    if plan.status != "approved" {
        return Err("必须先确认队长计划，才能启动 Worker".into());
    }
    let run = get_workspace_run_core(&db, &run_id)?;
    if run.status == "running" {
        return Err("该 Team run 已经在运行".into());
    }
    seed_worker_runs(&db, &run_id, &plan)?;
    let db_clone = Arc::clone(&db);
    let runtime_clone = Arc::clone(&runtime);
    let agent_clone = Arc::clone(&agent_manager);
    let scheduler_run_id = run_id.clone();
    tauri::async_runtime::spawn(async move {
        run_scheduler(
            db_clone,
            runtime_clone,
            agent_clone,
            scheduler_run_id,
            concurrency.unwrap_or(2).clamp(1, 4),
        )
        .await;
    });
    get_detail(&db, &run_id)
}

#[tauri::command]
pub async fn team_stop_run(
    run_id: String,
    db: State<'_, Arc<DbManager>>,
    runtime: State<'_, Arc<RuntimeManager>>,
) -> Result<TeamRunDetail, String> {
    let workers = list_agent_runs_core(&db, &run_id)?;
    for worker in &workers {
        if matches!(
            worker.status.as_str(),
            "running" | "awaiting_approval" | "retrying"
        ) {
            if let Some(session_id) = &worker.session_id {
                let _ = runtime.stop_session(session_id).await;
            }
        }
    }
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE agent_runs SET status = 'cancelled', completed_at = CURRENT_TIMESTAMP
         WHERE run_id = ?1 AND status IN ('queued','running','awaiting_approval','retrying')",
        params![run_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE workspace_runs SET status = 'cancelled', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    ).map_err(|error| error.to_string())?;
    drop(conn);
    get_detail(&db, &run_id)
}

fn requeue_worker_core(db: &DbManager, worker_id: &str) -> Result<String, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let run_id = conn
        .query_row(
            "SELECT run_id FROM agent_runs WHERE id = ?1 AND status IN ('failed','blocked')",
            params![worker_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "只有失败或阻塞的 Worker 可以重试".to_string())?;
    conn.execute(
        "UPDATE agent_runs SET status = 'queued', completed_at = NULL, validation_status = 'pending' WHERE id = ?1",
        params![worker_id],
    ).map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE agent_runs SET status = 'queued', completed_at = NULL, validation_status = 'pending'
         WHERE run_id = ?1 AND status = 'blocked'",
        params![run_id],
    ).map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE workspace_runs SET status = 'running', summary = '', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    ).map_err(|error| error.to_string())?;
    Ok(run_id)
}

#[tauri::command]
pub async fn team_retry_worker(
    worker_id: String,
    db: State<'_, Arc<DbManager>>,
    runtime: State<'_, Arc<RuntimeManager>>,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<TeamRunDetail, String> {
    let run_id = requeue_worker_core(&db, &worker_id)?;
    let scheduler_run_id = run_id.clone();
    let db_clone = Arc::clone(&db);
    let runtime_clone = Arc::clone(&runtime);
    let agent_clone = Arc::clone(&agent_manager);
    tauri::async_runtime::spawn(async move {
        run_scheduler(db_clone, runtime_clone, agent_clone, scheduler_run_id, 2).await;
    });
    get_detail(&db, &run_id)
}

#[tauri::command]
pub async fn team_respond_worker_approval(
    worker_id: String,
    request_id: String,
    approved: bool,
    requested_permissions: Option<serde_json::Value>,
    db: State<'_, Arc<DbManager>>,
    runtime: State<'_, Arc<RuntimeManager>>,
) -> Result<TeamRunDetail, String> {
    let (run_id, session_id, status): (String, String, String) = {
        let conn = db.get_connection().map_err(|error| error.to_string())?;
        conn.query_row(
            "SELECT run_id, session_id, status FROM agent_runs WHERE id = ?1",
            params![worker_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?
    };
    if status != "awaiting_approval" {
        return Err("该 Worker 当前没有等待审批".into());
    }
    runtime
        .respond_approval(
            &session_id,
            &request_id,
            approved,
            false,
            if approved { "accept" } else { "decline" },
            requested_permissions,
        )
        .await?;
    update_worker(
        &db,
        &worker_id,
        if approved { "running" } else { "failed" },
        Some(&session_id),
        if approved {
            None
        } else {
            Some("用户拒绝了 Worker 权限请求")
        },
        false,
    )?;
    get_detail(&db, &run_id)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusqlite::params;

    use crate::db::DbManager;

    use super::{
        parse_manager_plan, requeue_worker_core, seed_worker_runs, store_plan,
        validate_assignments, TeamAssignment,
    };
    use crate::commands::runs::{create_workspace_run_core, list_agent_runs_core};

    fn team_test_db(name: &str) -> Arc<DbManager> {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        Arc::new(DbManager::new_run_test(path))
    }

    #[test]
    fn manager_plan_rejects_cycles() {
        let raw = r#"{"assignments":[
          {"id":"a","agent_name":"Codex","task_title":"A","depends_on":["b"],"acceptance_criteria":[],"max_retries":1},
          {"id":"b","agent_name":"Claude Code","task_title":"B","depends_on":["a"],"acceptance_criteria":[],"max_retries":1}
        ]}"#;
        assert!(parse_manager_plan(raw).unwrap_err().contains("循环依赖"));
    }

    #[test]
    fn manager_plan_accepts_supported_dependency_graph() {
        let raw = r#"{"assignments":[
          {"id":"a","agent_name":"Codex","task_title":"Implement","depends_on":[],"acceptance_criteria":["tests pass"],"max_retries":1},
          {"id":"b","agent_name":"Claude Code","task_title":"Review","depends_on":["a"],"acceptance_criteria":["reviewed"],"max_retries":0}
        ]}"#;
        let plan = parse_manager_plan(raw).expect("valid plan");
        validate_assignments(&plan).expect("valid graph");
        assert_eq!(plan[1].depends_on, vec!["a"]);
    }

    #[test]
    fn retry_requeues_blocked_dependents() {
        let db = team_test_db("omnix_team_retry_test.sqlite");
        let run = create_workspace_run_core(&db, "retry", "D:/workspace", "Codex")
            .expect("run should be created");
        let assignments = vec![
            TeamAssignment {
                id: "a".into(),
                agent_name: "Codex".into(),
                task_title: "Implement".into(),
                depends_on: vec![],
                acceptance_criteria: vec!["passes".into()],
                max_retries: 1,
                status: "queued".into(),
            },
            TeamAssignment {
                id: "b".into(),
                agent_name: "Claude Code".into(),
                task_title: "Review".into(),
                depends_on: vec!["a".into()],
                acceptance_criteria: vec!["reviewed".into()],
                max_retries: 1,
                status: "queued".into(),
            },
        ];
        store_plan(&db, &run.id, "retry", &assignments).expect("plan should store");
        let plan =
            crate::commands::runs::get_team_plan_core(&db, &run.id).expect("plan should load");
        seed_worker_runs(&db, &run.id, &plan).expect("workers should seed");
        let workers = list_agent_runs_core(&db, &run.id).expect("workers should list");
        let first = workers
            .iter()
            .find(|worker| worker.assignment_id == "a")
            .unwrap();
        let second = workers
            .iter()
            .find(|worker| worker.assignment_id == "b")
            .unwrap();
        let conn = db.get_connection().unwrap();
        conn.execute(
            "UPDATE agent_runs SET status = 'failed' WHERE id = ?1",
            params![first.id],
        )
        .unwrap();
        conn.execute(
            "UPDATE agent_runs SET status = 'blocked' WHERE id = ?1",
            params![second.id],
        )
        .unwrap();
        drop(conn);

        requeue_worker_core(&db, &first.id).expect("retry should requeue graph");

        let workers = list_agent_runs_core(&db, &run.id).expect("workers should list");
        assert!(workers.iter().all(|worker| worker.status == "queued"));
    }
}
