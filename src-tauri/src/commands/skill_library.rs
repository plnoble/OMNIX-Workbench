use tauri::State;
use std::sync::Arc;
use std::path::PathBuf;
use rusqlite::params;
use crate::db::DbManager;
use crate::skill_library::{SkillMatch, SandboxResult, ProtocolAction, MarketSkill, DistillRecommendation};

/// 1. Semantic Skill Auto-Injection — find skills matching a message
#[tauri::command]
pub fn match_skills_for_injection(
    message: String,
    db: State<'_, Arc<DbManager>>,
) -> Vec<SkillMatch> {
    crate::skill_library::match_skills_for_message(&db, &message)
}

/// 2. Sandbox Testing — test a skill adversarially
#[tauri::command]
pub async fn test_skill_sandbox(
    skill_name: String,
    db: State<'_, std::sync::Arc<DbManager>>,
) -> Result<SandboxResult, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    // Read skill content
    let file_path: String = conn.query_row(
        "SELECT file_path FROM skills WHERE name = ?1",
        params![skill_name],
        |r| r.get(0),
    ).map_err(|e| format!("Skill not found: {}", e))?;

    let core_path = PathBuf::from(&file_path).join(format!("{}_core.md", skill_name));
    let skill_content = std::fs::read_to_string(&core_path)
        .map_err(|e| format!("Failed to read skill: {}", e))?;

    // Generate test cases
    let test_cases = crate::skill_library::generate_test_cases(&skill_name, &skill_content);

    // For now, return the test structure without actual LLM calls
    let scores = test_cases.iter().map(|tc| {
        crate::skill_library::TestCaseScore {
            input: tc.input.clone(),
            agent_response: "(Sandbox test requires LLM connection)".into(),
            auditor_score: 0,
            auditor_feedback: "Test not executed — connect an LLM provider first".into(),
        }
    }).collect::<Vec<_>>();

    Ok(SandboxResult {
        skill_name,
        test_cases_total: test_cases.len(),
        test_cases_passed: 0,
        average_score: 0.0,
        scores,
        overall_verdict: "Pending — requires LLM connection".into(),
    })
}

/// 3. Text Protocol Interception — parse protocol blocks from AI output
#[tauri::command]
pub fn intercept_protocols(output: String) -> Vec<ProtocolAction> {
    crate::skill_library::intercept_protocols(&output)
}

/// Execute a protocol action
#[tauri::command]
pub fn execute_protocol(
    action: ProtocolAction,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    crate::skill_library::execute_protocol_action(&action, &db)
}

/// 4. Multi-Source Market Search
#[tauri::command]
pub async fn search_skill_market(query: String) -> Result<Vec<MarketSkill>, String> {
    crate::skill_library::search_market(&query).await
}

/// 5. Experience Distillation — analyze project and recommend skills
#[tauri::command]
pub fn distill_from_project(project_path: String) -> Result<Vec<DistillRecommendation>, String> {
    crate::skill_library::distill_from_project(&project_path)
}
