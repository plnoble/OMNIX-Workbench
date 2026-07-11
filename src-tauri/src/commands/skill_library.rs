use crate::db::DbManager;
use crate::skill_library::{
    DistillRecommendation, MarketSkill, MarketSkillPreview, ProtocolAction, SandboxResult,
    SkillMatch,
};
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// 1. Semantic Skill Auto-Injection — find skills matching a message.
/// `official_only` restricts to the 正式池 (what the gateway would inject).
#[tauri::command]
pub fn match_skills_for_injection(
    message: String,
    official_only: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Vec<SkillMatch> {
    crate::skill_library::match_skills_for_message(&db, &message, official_only.unwrap_or(false))
}

/// 2. Sandbox Testing — test a skill adversarially
#[tauri::command]
pub async fn test_skill_sandbox(
    skill_name: String,
    db: State<'_, std::sync::Arc<DbManager>>,
) -> Result<SandboxResult, String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;

    // Read skill content
    let file_path: String = conn
        .query_row(
            "SELECT file_path FROM skills WHERE name = ?1",
            params![skill_name],
            |r| r.get(0),
        )
        .map_err(|e| format!("Skill not found: {}", e))?;

    let core_path = PathBuf::from(&file_path).join(format!("{}_core.md", skill_name));
    let skill_content =
        std::fs::read_to_string(&core_path).map_err(|e| format!("Failed to read skill: {}", e))?;

    // Generate test cases
    let test_cases = crate::skill_library::generate_test_cases(&skill_name, &skill_content);

    // For now, return the test structure without actual LLM calls
    let scores = test_cases
        .iter()
        .map(|tc| crate::skill_library::TestCaseScore {
            input: tc.input.clone(),
            agent_response: "(Sandbox test requires LLM connection)".into(),
            auditor_score: 0,
            auditor_feedback: "Test not executed — connect an LLM provider first".into(),
        })
        .collect::<Vec<_>>();

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

#[tauri::command]
pub async fn preview_market_skill(skill: MarketSkill) -> Result<MarketSkillPreview, String> {
    crate::skill_library::fetch_market_skill(&skill).await
}

#[tauri::command]
pub async fn import_market_skill(
    skill: MarketSkill,
    overwrite: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let preview = crate::skill_library::fetch_market_skill(&skill).await?;
    let safe_name = skill
        .name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if safe_name.is_empty() {
        return Err("市场技能名称无效".into());
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM skills WHERE name = ?1",
            params![safe_name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists > 0 && !overwrite.unwrap_or(false) {
        return Err(format!("技能 {safe_name} 已存在，请明确选择覆盖"));
    }

    let central_dir = crate::storage::skills_dir().join(&safe_name);
    std::fs::create_dir_all(&central_dir).map_err(|e| e.to_string())?;
    for file_name in [
        "SKILL.md".to_string(),
        format!("{safe_name}_core.md"),
        format!("{safe_name}_minimal.md"),
        format!("{safe_name}_comprehensive.md"),
    ] {
        std::fs::write(central_dir.join(file_name), &preview.content)
            .map_err(|e| format!("写入技能失败: {e}"))?;
    }

    let description = crate::skill_frontmatter::parse_frontmatter(&preview.content)
        .0
        .description
        .unwrap_or_else(|| skill.description.clone());
    let central_path = central_dir.to_string_lossy().to_string();
    conn.execute(
        "INSERT OR REPLACE INTO skills
         (name, description, file_path, profile, is_active, dependencies, source_type,
          source_ref, source_revision, central_path, content_hash)
         VALUES (?1, ?2, ?3, 'Core', 1, '[]', 'market', ?4, ?5, ?3, ?6)",
        params![
            safe_name,
            description,
            central_path,
            skill.repo_url,
            if skill.content_sha.is_empty() {
                skill.revision
            } else {
                skill.content_sha
            },
            preview.content_hash,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(safe_name)
}

/// 5. Experience Distillation — analyze project and recommend skills
#[tauri::command]
pub fn distill_from_project(project_path: String) -> Result<Vec<DistillRecommendation>, String> {
    crate::skill_library::distill_from_project(&project_path)
}
