use super::*;
use crate::db::DbManager;
use crate::input_validation;
use crate::skill_frontmatter::{generate_with_frontmatter, parse_frontmatter, SkillFrontmatter};
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_all_skills(db: State<'_, Arc<DbManager>>) -> Result<Vec<Skill>, String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT name, description, file_path, profile, is_active, dependencies, updated_at, \
         COALESCE(source_type,'local'), source_ref, source_revision, \
         COALESCE(central_path,''), content_hash, starred, category \
         FROM skills",
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt
        .query_map([], |row: &rusqlite::Row| {
            let name: String = row.get(0)?;
            let description: String = row.get(1)?;
            let file_path: String = row.get(2)?;
            let profile: String = row.get(3)?;
            let is_active_int: i32 = row.get(4)?;
            let dependencies_str: String = row.get(5)?;
            let updated_at: String = row.get(6)?;
            let source_type: String = row.get(7)?;
            let source_ref: Option<String> = row.get(8)?;
            let source_revision: Option<String> = row.get(9)?;
            let central_path: String = row.get(10)?;
            let content_hash: Option<String> = row.get(11)?;
            let starred_int: i32 = row.get(12)?;
            let category: Option<String> = row.get(13)?;

            let dependencies: Vec<String> =
                serde_json::from_str(&dependencies_str).unwrap_or_default();

            Ok(Skill {
                name,
                description,
                file_path,
                profile,
                is_active: is_active_int != 0,
                dependencies,
                updated_at,
                source_type,
                source_ref,
                source_revision,
                central_path,
                content_hash,
                starred: starred_int != 0,
                category,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(skill) = r {
            result.push(skill);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn get_skill_content(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt
        .query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;

    let mut path = PathBuf::from(&file_path_str);

    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));

    if !path.exists() {
        return Err(format!(
            "Profile file not found at: {}",
            path.to_string_lossy()
        ));
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read skill content: {}", e))?;

    // Parse frontmatter and return body only (frontend gets clean content)
    let (_fm, body) = parse_frontmatter(&raw);
    Ok(body)
}

#[tauri::command]
pub fn save_skill_content(
    name: String,
    profile: String,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt
        .query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;

    let mut path = PathBuf::from(&file_path_str);

    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));

    let mut tmp_path = path.clone();
    tmp_path.set_extension("tmp");

    std::fs::write(&tmp_path, &content)
        .map_err(|e| format!("Failed to write temporary file: {}", e))?;

    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to atomically replace skill file: {}", e))?;

    conn.execute(
        "UPDATE skills SET updated_at = CURRENT_TIMESTAMP WHERE name = ?1",
        params![name],
    )
    .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn toggle_skill_active(
    name: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET is_active = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![if is_active { 1 } else { 0 }, name],
    )
    .map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_skill_profile(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET profile = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![profile, name],
    )
    .map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn fuse_skills_api(
    skills: Vec<String>,
    model_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillFusionResult, String> {
    if skills.len() < 2 {
        return Err("Please select at least 2 skills to fuse.".to_string());
    }

    let model_id = model_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "请先从 Models 中选择一个启用模型".to_string())?;
    crate::knowledge::resolve_chat_platform(&db, &model_id)?;

    let mut skills_payload = String::new();
    for name in &skills {
        let file_content = {
            let conn = db
                .get_connection()
                .map_err(|e: rusqlite::Error| e.to_string())?;
            let mut stmt = conn
                .prepare("SELECT file_path, COALESCE(central_path, '') FROM skills WHERE name = ?1")
                .map_err(|e: rusqlite::Error| e.to_string())?;
            let (file_path_str, central_path): (String, String) = stmt
                .query_row(params![name], |r: &rusqlite::Row| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .map_err(|_: rusqlite::Error| format!("Skill {} path not found", name))?;
            let base = if central_path.is_empty() {
                PathBuf::from(file_path_str)
            } else {
                PathBuf::from(central_path)
            };
            let candidates = [
                base.join("SKILL.md"),
                base.join(format!("{}_core.md", name)),
            ];
            let path = candidates
                .iter()
                .find(|path| path.is_file())
                .ok_or_else(|| format!("技能 {} 缺少 SKILL.md/Core 内容", name))?;
            std::fs::read_to_string(path)
                .map_err(|_| format!("Failed to read core profile of skill {}", name))?
        };

        skills_payload.push_str(&format!("\n=== SKILL: {} ===\n{}\n", name, file_content));
    }

    let system_prompt = "You are a Meta-Evolution Engine designed to analyze and fuse AI agent programming guidelines and skills. \
You must merge the input skills into a single consolidated, ultra-optimized super skill without conflicts. \
Your output must be returned strictly as a JSON object with the following schema:
{
  \"name\": \"Fused Skill Name\",
  \"description\": \"Brief description of the fused super skill\",
  \"fused_code\": \"The full markdown code representing the fused skill. Make sure it contains markdown sections: # Role & Identity, # Core Knowledge, # Step-by-Step Workflow, # Quality Checklist, # Anti-Patterns\",
  \"explanation\": \"A short description of how the skills were merged and what conflicts were resolved\",
  \"conflicts\": [\"One explicit conflict or tradeoff per item\"]
}
DO NOT output any wrapping markdown blocks like ```json outside of the raw JSON content.";

    let user_prompt = format!(
        "Please fuse the following skills into a single unified skill asset:\n{}\n\nRemember to return a valid JSON object matching the requested schema.",
        skills_payload
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let proxy_port = db
        .get_setting("proxy_port")
        .ok()
        .flatten()
        .unwrap_or_else(|| "1421".into());
    let upstream_url = format!("http://127.0.0.1:{proxy_port}/v1/chat/completions");

    let response = client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model_id,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3,
            "response_format": {"type": "json_object"}
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to connect to LLM upstream: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Upstream LLM returned error ({}): {}",
            status, err_body
        ));
    }

    let res_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response from LLM: {}", e))?;

    let text_content = res_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "Failed to retrieve content from LLM response".to_string())?;

    let clean_json_str = text_content
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();

    #[derive(serde::Deserialize)]
    struct FusionLlmResult {
        name: String,
        description: String,
        fused_code: String,
        explanation: String,
        #[serde(default)]
        conflicts: Vec<String>,
    }

    let result: FusionLlmResult = serde_json::from_str(clean_json_str).map_err(|e| {
        format!(
            "LLM output did not match expected JSON schema: {}. Raw output: {}",
            e, clean_json_str
        )
    })?;
    let draft_id = format!("fusion_{}", chrono::Utc::now().timestamp_micros());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO skill_fusion_drafts
         (id, source_skills_json, model_id, proposed_name, description, fused_content,
          explanation, conflicts_json, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
        params![
            draft_id,
            serde_json::to_string(&skills).map_err(|e| e.to_string())?,
            model_id,
            result.name,
            result.description,
            result.fused_code,
            result.explanation,
            serde_json::to_string(&result.conflicts).map_err(|e| e.to_string())?,
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(SkillFusionResult {
        draft_id,
        name: result.name,
        description: result.description,
        fused_code: result.fused_code,
        explanation: result.explanation,
        conflicts: result.conflicts,
        status: "pending".into(),
    })
}

pub(crate) fn create_skill_core(
    db: &DbManager,
    name: &str,
    description: &str,
    profile: &str,
    dependencies: &[String],
    content: &str,
    overwrite: bool,
) -> Result<(), String> {
    input_validation::validate_name(name, "name")?;
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM skills WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists > 0 && !overwrite {
        return Err(format!("技能 {name} 已存在"));
    }

    let home_dir = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let base_path = home_dir.join(".omnix").join("skills").join(name);
    std::fs::create_dir_all(&base_path).map_err(|e| e.to_string())?;
    let base_path_str = base_path.to_string_lossy().to_string();
    let frontmatter = SkillFrontmatter {
        name: Some(name.to_string()),
        description: Some(description.to_string()),
        category: Some("Custom".into()),
        version: Some("1.0.0".into()),
        skills: if dependencies.is_empty() {
            None
        } else {
            Some(dependencies.to_vec())
        },
        ..Default::default()
    };
    let content_with_frontmatter = generate_with_frontmatter(&frontmatter, content);
    for file_name in [
        "SKILL.md".to_string(),
        format!("{name}_core.md"),
        format!("{name}_minimal.md"),
        format!("{name}_comprehensive.md"),
    ] {
        let target = base_path.join(file_name);
        let temporary = target.with_extension("tmp");
        std::fs::write(&temporary, &content_with_frontmatter).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
    }
    let dependencies_json = serde_json::to_string(dependencies).map_err(|e| e.to_string())?;
    let content_hash = crate::hash::fnv1a_hash(&content_with_frontmatter);
    conn.execute(
        "INSERT OR REPLACE INTO skills
         (name, description, file_path, profile, is_active, dependencies, source_type,
          central_path, content_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, 'local', ?3, ?6, CURRENT_TIMESTAMP)",
        params![
            name,
            description,
            base_path_str,
            profile,
            dependencies_json,
            content_hash
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn create_skill(
    name: String,
    description: String,
    profile: String,
    dependencies: Vec<String>,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    create_skill_core(
        &db,
        &name,
        &description,
        &profile,
        &dependencies,
        &content,
        false,
    )
}

#[tauri::command]
pub fn apply_skill_fusion_draft(
    draft_id: String,
    approved_name: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let (description, content, sources_json, status): (String, String, String, String) = conn
        .query_row(
            "SELECT description, fused_content, source_skills_json, status
             FROM skill_fusion_drafts WHERE id = ?1",
            params![draft_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| format!("融合草案不存在: {e}"))?;
    if status != "pending" {
        return Err(format!("融合草案当前状态为 {status}，不能重复应用"));
    }
    let dependencies: Vec<String> = serde_json::from_str(&sources_json).unwrap_or_default();
    drop(conn);
    create_skill_core(
        &db,
        approved_name.trim(),
        &description,
        "Core",
        &dependencies,
        &content,
        false,
    )?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE skill_fusion_drafts SET status = 'approved', applied_at = CURRENT_TIMESTAMP WHERE id = ?1",
        params![draft_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn reject_skill_fusion_draft(
    draft_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE skill_fusion_drafts SET status = 'rejected' WHERE id = ?1 AND status = 'pending'",
            params![draft_id],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        Err("没有可拒绝的融合草案".into())
    } else {
        Ok(())
    }
}
