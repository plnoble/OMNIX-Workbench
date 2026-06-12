use tauri::State;
use std::sync::Arc;
use std::path::PathBuf;
use rusqlite::params;
use crate::db::DbManager;
use crate::skill_frontmatter::{SkillFrontmatter, generate_with_frontmatter, parse_frontmatter};
use crate::input_validation;
use super::*;

#[tauri::command]
pub fn get_all_skills(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Skill>, String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT name, description, file_path, profile, is_active, dependencies, updated_at, \
         COALESCE(source_type,'local'), source_ref, source_revision, \
         COALESCE(central_path,''), content_hash, starred, category \
         FROM skills"
    )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt.query_map([], |row: &rusqlite::Row| {
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

        let dependencies: Vec<String> = serde_json::from_str(&dependencies_str).unwrap_or_default();

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
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    
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
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt.query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;

    let mut path = PathBuf::from(&file_path_str);

    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));

    if !path.exists() {
        return Err(format!("Profile file not found at: {}", path.to_string_lossy()));
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
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn.prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt.query_row(params![name], |r: &rusqlite::Row| r.get(0))
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
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub fn toggle_skill_active(
    name: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET is_active = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![if is_active { 1 } else { 0 }, name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_skill_profile(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET profile = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![profile, name],
    ).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn fuse_skills_api(
    skills: Vec<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<SkillFusionResult, String> {
    if skills.len() < 2 {
        return Err("Please select at least 2 skills to fuse.".to_string());
    }

    let api_key_raw = db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return Err("API Key is not configured. Please set it in Settings.".to_string());
    }
    let api_key = keys[0];
    
    let api_host = db.get_setting("api_host").unwrap_or(None)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let target_model = db.get_setting("target_model").unwrap_or(None)
        .unwrap_or_else(|| "deepseek-chat".to_string());

    let mut skills_payload = String::new();
    for name in &skills {
        let file_content = {
            let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;
            let mut stmt = conn.prepare("SELECT file_path FROM skills WHERE name = ?1").map_err(|e: rusqlite::Error| e.to_string())?;
            let file_path_str: String = stmt.query_row(params![name], |r: &rusqlite::Row| r.get(0)).map_err(|_: rusqlite::Error| format!("Skill {} path not found", name))?;
            let mut path = PathBuf::from(file_path_str);
            path.set_file_name(format!("{}_core.md", name));
            std::fs::read_to_string(&path).map_err(|_| format!("Failed to read core profile of skill {}", name))?
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
  \"explanation\": \"A short description of how the skills were merged and what conflicts were resolved\"
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
    let upstream_url = format!("{}/chat/completions", api_host.trim_end_matches('/'));
    
    let response = client.post(&upstream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": target_model,
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
        return Err(format!("Upstream LLM returned error ({}): {}", status, err_body));
    }

    let res_json: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse JSON response from LLM: {}", e))?;

    let text_content = res_json["choices"][0]["message"]["content"].as_str()
        .ok_or_else(|| "Failed to retrieve content from LLM response".to_string())?;

    let clean_json_str = text_content
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();

    let result: SkillFusionResult = serde_json::from_str(clean_json_str)
        .map_err(|e| format!("LLM output did not match expected JSON schema: {}. Raw output: {}", e, clean_json_str))?;

    Ok(result)
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
    input_validation::validate_name(&name, "name")?;
    let conn = db.get_connection().map_err(|e: rusqlite::Error| e.to_string())?;

    let home_dir = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let mut skills_dir = home_dir.clone();
    skills_dir.push(".omnix");
    skills_dir.push("skills");
    if !skills_dir.exists() {
        let _ = std::fs::create_dir_all(&skills_dir).map_err(|e| e.to_string())?;
    }

    let mut base_path = skills_dir.clone();
    base_path.push(&name);
    let base_path_str = base_path.to_string_lossy().to_string();

    // Generate content with YAML frontmatter (Multica-inspired)
    let frontmatter = SkillFrontmatter {
        name: Some(name.clone()),
        description: Some(description.clone()),
        category: Some("Custom".into()),
        version: Some("1.0.0".into()),
        skills: if dependencies.is_empty() { None } else { Some(dependencies.clone()) },
        ..Default::default()
    };
    let content_with_frontmatter = generate_with_frontmatter(&frontmatter, &content);

    // Write target profile files with frontmatter
    let mut core_path = base_path.clone();
    core_path.set_file_name(format!("{}_core.md", name));
    std::fs::write(&core_path, &content_with_frontmatter).map_err(|e| e.to_string())?;

    let mut min_path = base_path.clone();
    min_path.set_file_name(format!("{}_minimal.md", name));
    std::fs::write(&min_path, &content_with_frontmatter).map_err(|e| e.to_string())?;

    let mut comp_path = base_path.clone();
    comp_path.set_file_name(format!("{}_comprehensive.md", name));
    std::fs::write(&comp_path, &content_with_frontmatter).map_err(|e| e.to_string())?;

    // Also write SKILL.md with frontmatter
    let skill_md_path = base_path.join("SKILL.md");
    std::fs::write(&skill_md_path, &content_with_frontmatter).map_err(|e| e.to_string())?;

    let deps_str = serde_json::to_string(&dependencies).unwrap_or_else(|_| "[]".to_string());

    conn.execute(
        "INSERT OR REPLACE INTO skills (name, description, file_path, profile, is_active, dependencies)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![name, description, base_path_str, profile, 1, deps_str],
    ).map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}
