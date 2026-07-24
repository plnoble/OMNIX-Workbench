use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::input_validation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRun {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub manager_agent: String,
    pub status: String,
    pub summary: String,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub run_id: String,
    pub agent_name: String,
    pub task_title: String,
    pub status: String,
    pub session_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub log_excerpt: String,
    pub assignment_id: String,
    pub dependencies: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub result_summary: String,
    pub validation_status: String,
    #[serde(default = "default_work_mode")]
    pub work_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamAssignmentInput {
    pub agent_name: String,
    pub task_title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: i64,
    #[serde(default = "default_work_mode")]
    pub work_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamAssignment {
    pub id: String,
    pub agent_name: String,
    pub task_title: String,
    pub status: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: i64,
    #[serde(default = "default_work_mode")]
    pub work_mode: String,
}

fn default_max_retries() -> i64 {
    1
}

/// 编排预设的 worker 工作模式："direct"（可写，默认，队长实现型）或 "plan"
/// （只读/计划，顾问与委员会用——给意见不动文件）。
pub fn default_work_mode() -> String {
    "direct".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamPlan {
    pub run_id: String,
    pub goal: String,
    pub assignments: Vec<TeamAssignment>,
    pub status: String,
    pub created_at: String,
    pub approved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabFeature {
    pub id: String,
    pub title: String,
    pub layer: String,
    pub status: String,
    pub risk: String,
    pub description: String,
    pub is_visible: bool,
}

fn next_id(prefix: &str) -> String {
    format!("{}_{}", prefix, Utc::now().timestamp_micros())
}

pub fn create_workspace_run_core(
    db: &Arc<DbManager>,
    title: &str,
    workspace_path: &str,
    manager_agent: &str,
) -> Result<WorkspaceRun, String> {
    input_validation::validate_content(title, "title")?;
    input_validation::validate_workspace_path(workspace_path, "workspace_path")?;
    input_validation::validate_name(manager_agent, "manager_agent")?;
    if title.trim().is_empty() {
        return Err("title must not be empty".into());
    }

    let id = next_id("run");
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO workspace_runs (id, title, workspace_path, manager_agent, status)
         VALUES (?1, ?2, ?3, ?4, 'draft')",
        params![
            id,
            title.trim(),
            workspace_path.trim(),
            manager_agent.trim()
        ],
    )
    .map_err(|e| e.to_string())?;

    get_workspace_run_core(db, &id)
}

pub fn get_workspace_run_core(db: &Arc<DbManager>, run_id: &str) -> Result<WorkspaceRun, String> {
    input_validation::validate_id(run_id, "run_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, title, workspace_path, manager_agent, status, summary, is_archived, created_at, updated_at
         FROM workspace_runs WHERE id = ?1",
        params![run_id],
        |row| {
            Ok(WorkspaceRun {
                id: row.get(0)?,
                title: row.get(1)?,
                workspace_path: row.get(2)?,
                manager_agent: row.get(3)?,
                status: row.get(4)?,
                summary: row.get(5)?,
                is_archived: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

pub fn list_workspace_runs_core(
    db: &Arc<DbManager>,
    include_archived: bool,
) -> Result<Vec<WorkspaceRun>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let sql = if include_archived {
        "SELECT id, title, workspace_path, manager_agent, status, summary, is_archived, created_at, updated_at
         FROM workspace_runs ORDER BY created_at DESC"
    } else {
        "SELECT id, title, workspace_path, manager_agent, status, summary, is_archived, created_at, updated_at
         FROM workspace_runs WHERE is_archived = 0 ORDER BY created_at DESC"
    };

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(WorkspaceRun {
                id: row.get(0)?,
                title: row.get(1)?,
                workspace_path: row.get(2)?,
                manager_agent: row.get(3)?,
                status: row.get(4)?,
                summary: row.get(5)?,
                is_archived: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn propose_team_plan_core(
    db: &Arc<DbManager>,
    run_id: &str,
    goal: &str,
    assignments: Vec<(String, String)>,
) -> Result<TeamPlan, String> {
    input_validation::validate_id(run_id, "run_id")?;
    input_validation::validate_content(goal, "goal")?;
    if goal.trim().is_empty() {
        return Err("goal must not be empty".into());
    }
    if assignments.is_empty() {
        return Err("assignments must not be empty".into());
    }

    let normalized: Vec<TeamAssignment> = assignments
        .into_iter()
        .enumerate()
        .map(|(index, (agent_name, task_title))| TeamAssignment {
            id: format!("assign_{}", index + 1),
            agent_name,
            task_title,
            status: "planned".into(),
            depends_on: Vec::new(),
            acceptance_criteria: Vec::new(),
            max_retries: 1,
            work_mode: default_work_mode(),
        })
        .collect();

    for assignment in &normalized {
        input_validation::validate_name(&assignment.agent_name, "agent_name")?;
        input_validation::validate_content(&assignment.task_title, "task_title")?;
        if assignment.task_title.trim().is_empty() {
            return Err("task_title must not be empty".into());
        }
    }

    let assignments_json = serde_json::to_string(&normalized).map_err(|e| e.to_string())?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO team_plans (run_id, goal, assignments_json, status, approved_at)
         VALUES (?1, ?2, ?3, 'proposed', NULL)",
        params![run_id, goal.trim(), assignments_json],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE workspace_runs SET status = 'planning', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    )
    .map_err(|e| e.to_string())?;

    get_team_plan_core(db, run_id)
}

pub fn get_team_plan_core(db: &Arc<DbManager>, run_id: &str) -> Result<TeamPlan, String> {
    input_validation::validate_id(run_id, "run_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT run_id, goal, assignments_json, status, created_at, approved_at
         FROM team_plans WHERE run_id = ?1",
        params![run_id],
        |row| {
            let assignments_json: String = row.get(2)?;
            let assignments = serde_json::from_str(&assignments_json).unwrap_or_default();
            Ok(TeamPlan {
                run_id: row.get(0)?,
                goal: row.get(1)?,
                assignments,
                status: row.get(3)?,
                created_at: row.get(4)?,
                approved_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

pub fn approve_team_plan_core(db: &Arc<DbManager>, run_id: &str) -> Result<TeamPlan, String> {
    input_validation::validate_id(run_id, "run_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let existing: Option<String> = conn
        .query_row(
            "SELECT status FROM team_plans WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if existing.is_none() {
        return Err("team plan not found".into());
    }

    conn.execute(
        "UPDATE team_plans SET status = 'approved', approved_at = CURRENT_TIMESTAMP WHERE run_id = ?1",
        params![run_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE workspace_runs SET status = 'approved', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![run_id],
    )
    .map_err(|e| e.to_string())?;

    get_team_plan_core(db, run_id)
}

pub fn start_agent_run_core(
    db: &Arc<DbManager>,
    run_id: &str,
    agent_name: &str,
    task_title: &str,
    status: &str,
) -> Result<AgentRun, String> {
    input_validation::validate_id(run_id, "run_id")?;
    input_validation::validate_name(agent_name, "agent_name")?;
    input_validation::validate_content(task_title, "task_title")?;
    input_validation::validate_name(status, "status")?;
    if task_title.trim().is_empty() {
        return Err("task_title must not be empty".into());
    }

    let id = next_id("agent_run");
    let session_id = format!("{}_{}", run_id, agent_name.replace(' ', "_").to_lowercase());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO agent_runs (id, run_id, agent_name, task_title, status, session_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            run_id,
            agent_name.trim(),
            task_title.trim(),
            status.trim(),
            session_id
        ],
    )
    .map_err(|e| e.to_string())?;

    get_agent_run_core(db, &id)
}

pub fn get_agent_run_core(db: &Arc<DbManager>, agent_run_id: &str) -> Result<AgentRun, String> {
    input_validation::validate_id(agent_run_id, "agent_run_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, run_id, agent_name, task_title, status, session_id, started_at, completed_at, log_excerpt,
                assignment_id, dependencies_json, acceptance_json, retry_count, max_retries, result_summary, validation_status, work_mode
         FROM agent_runs WHERE id = ?1",
        params![agent_run_id],
        |row| {
            Ok(AgentRun {
                id: row.get(0)?,
                run_id: row.get(1)?,
                agent_name: row.get(2)?,
                task_title: row.get(3)?,
                status: row.get(4)?,
                session_id: row.get(5)?,
                started_at: row.get(6)?,
                completed_at: row.get(7)?,
                log_excerpt: row.get(8)?,
                assignment_id: row.get(9)?,
                dependencies: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
                acceptance_criteria: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or_default(),
                retry_count: row.get(12)?,
                max_retries: row.get(13)?,
                result_summary: row.get(14)?,
                validation_status: row.get(15)?,
                work_mode: row.get(16)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

pub fn list_agent_runs_core(db: &Arc<DbManager>, run_id: &str) -> Result<Vec<AgentRun>, String> {
    input_validation::validate_id(run_id, "run_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, run_id, agent_name, task_title, status, session_id, started_at, completed_at, log_excerpt,
                    assignment_id, dependencies_json, acceptance_json, retry_count, max_retries, result_summary, validation_status, work_mode
             FROM agent_runs WHERE run_id = ?1 ORDER BY id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![run_id], |row| {
            Ok(AgentRun {
                id: row.get(0)?,
                run_id: row.get(1)?,
                agent_name: row.get(2)?,
                task_title: row.get(3)?,
                status: row.get(4)?,
                session_id: row.get(5)?,
                started_at: row.get(6)?,
                completed_at: row.get(7)?,
                log_excerpt: row.get(8)?,
                assignment_id: row.get(9)?,
                dependencies: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
                acceptance_criteria: serde_json::from_str(&row.get::<_, String>(11)?)
                    .unwrap_or_default(),
                retry_count: row.get(12)?,
                max_retries: row.get(13)?,
                result_summary: row.get(14)?,
                validation_status: row.get(15)?,
                work_mode: row.get(16)?,
            })
        })
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn list_lab_features_core() -> Vec<LabFeature> {
    vec![
        LabFeature {
            id: "compare".into(),
            title: "AI 专家比对".into(),
            layer: "labs".into(),
            status: "experimental".into(),
            risk: "medium".into(),
            description: "多模型并排评审和专家意见对照，保留但不作为主工作流入口。".into(),
            is_visible: true,
        },
        LabFeature {
            id: "cron".into(),
            title: "定时任务".into(),
            layer: "labs".into(),
            status: "experimental".into(),
            risk: "medium".into(),
            description: "后台定时执行 Agent 任务，等待与统一 run 模型进一步整合。".into(),
            is_visible: true,
        },
        LabFeature {
            id: "autopilot".into(),
            title: "自动驾驶".into(),
            layer: "labs".into(),
            status: "incomplete".into(),
            risk: "high".into(),
            description: "无人值守开发能力需要更多权限、确认队列和验证护栏。".into(),
            is_visible: true,
        },
        LabFeature {
            id: "skill-evolution".into(),
            title: "技能进化".into(),
            layer: "labs".into(),
            status: "experimental".into(),
            risk: "medium".into(),
            description: "技能融合、审计、自动修复和经验蒸馏先从正式包管理中拆出。".into(),
            is_visible: true,
        },
        LabFeature {
            id: "cookbook".into(),
            title: "模型推荐 Cookbook".into(),
            layer: "labs".into(),
            status: "experimental".into(),
            risk: "low".into(),
            description: "硬件画像、模型谱系和推荐能力先作为实验辅助工具。".into(),
            is_visible: true,
        },
        LabFeature {
            id: "code-analysis".into(),
            title: "代码库分析".into(),
            layer: "labs".into(),
            status: "experimental".into(),
            risk: "medium".into(),
            description: "架构图谱和代码深度分析后续接入 run 验证链路。".into(),
            is_visible: true,
        },
    ]
}

#[tauri::command]
pub fn create_workspace_run(
    title: String,
    workspace_path: String,
    manager_agent: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<WorkspaceRun, String> {
    create_workspace_run_core(&db, &title, &workspace_path, &manager_agent)
}

#[tauri::command]
pub fn list_workspace_runs(
    include_archived: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<WorkspaceRun>, String> {
    list_workspace_runs_core(&db, include_archived.unwrap_or(false))
}

#[tauri::command]
pub fn get_workspace_run(
    run_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<WorkspaceRun, String> {
    get_workspace_run_core(&db, &run_id)
}

#[tauri::command]
pub fn propose_team_plan(
    run_id: String,
    goal: String,
    assignments: Vec<TeamAssignmentInput>,
    db: State<'_, Arc<DbManager>>,
) -> Result<TeamPlan, String> {
    let pairs = assignments
        .into_iter()
        .map(|assignment| (assignment.agent_name, assignment.task_title))
        .collect();
    propose_team_plan_core(&db, &run_id, &goal, pairs)
}

#[tauri::command]
pub fn get_team_plan(run_id: String, db: State<'_, Arc<DbManager>>) -> Result<TeamPlan, String> {
    get_team_plan_core(&db, &run_id)
}

#[tauri::command]
pub fn approve_team_plan(
    run_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<TeamPlan, String> {
    approve_team_plan_core(&db, &run_id)
}

#[tauri::command]
pub fn start_agent_run(
    run_id: String,
    agent_name: String,
    task_title: String,
    status: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<AgentRun, String> {
    start_agent_run_core(
        &db,
        &run_id,
        &agent_name,
        &task_title,
        status.as_deref().unwrap_or("pending"),
    )
}

#[tauri::command]
pub fn list_agent_runs(
    run_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<AgentRun>, String> {
    list_agent_runs_core(&db, &run_id)
}

#[tauri::command]
pub fn list_lab_features() -> Vec<LabFeature> {
    list_lab_features_core()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::db::DbManager;

    use super::{
        approve_team_plan_core, create_workspace_run_core, list_lab_features_core,
        list_workspace_runs_core, propose_team_plan_core, start_agent_run_core,
    };

    fn run_test_db(name: &str) -> Arc<DbManager> {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        Arc::new(DbManager::new_run_test(path))
    }

    #[test]
    fn workspace_run_lifecycle_creates_lists_and_starts_agent_run() {
        let db = run_test_db("omnix_run_lifecycle_test.sqlite");

        let run = create_workspace_run_core(
            &db,
            "重构工作台",
            "D:/Agent/Project/OMNIX-Development Tools",
            "Claude Code",
        )
        .expect("run should be created");

        assert_eq!(run.status, "draft");
        assert_eq!(run.manager_agent, "Claude Code");

        let runs = list_workspace_runs_core(&db, false).expect("runs should list");
        assert!(runs.iter().any(|item| item.id == run.id));

        let agent_run = start_agent_run_core(&db, &run.id, "Codex", "实现团队任务 UI", "pending")
            .expect("agent run should be created");

        assert_eq!(agent_run.run_id, run.id);
        assert_eq!(agent_run.status, "pending");
    }

    #[test]
    fn manager_plan_requires_explicit_approval() {
        let db = run_test_db("omnix_run_plan_test.sqlite");

        let run = create_workspace_run_core(&db, "实现半自动队长", "D:/workspace", "Claude Code")
            .expect("run should be created");

        let plan = propose_team_plan_core(
            &db,
            &run.id,
            "把任务拆给 Claude、Codex、Gemini、OpenCode",
            vec![
                ("Claude Code".to_string(), "拆解需求并更新计划".to_string()),
                ("Codex".to_string(), "实现类型和 API".to_string()),
            ],
        )
        .expect("plan should be proposed");

        assert_eq!(plan.status, "proposed");
        assert_eq!(plan.assignments.len(), 2);

        let approved = approve_team_plan_core(&db, &run.id).expect("plan should approve");
        assert_eq!(approved.status, "approved");
        assert!(approved.approved_at.is_some());
    }

    #[test]
    fn labs_registry_marks_experimental_features_visible() {
        let features = list_lab_features_core();

        assert!(features.iter().any(|feature| {
            feature.id == "compare" && feature.layer == "labs" && feature.is_visible
        }));
        assert!(features.iter().any(|feature| {
            feature.id == "skill-evolution" && feature.status == "experimental"
        }));
    }
}
