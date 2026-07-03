use tauri::State;
use std::sync::Arc;
use rusqlite::params;
use crate::db::DbManager;
use crate::input_validation;
use super::*;

#[tauri::command]
pub fn get_all_memories(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Memory>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, incident_desc, code_pattern, remediation, keywords, created_at, \
                confidence, seen_count, repeated_count, status \
         FROM memories WHERE status IS NULL OR status != 'merged' ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(Memory {
            id: row.get(0)?,
            incident_desc: row.get(1)?,
            code_pattern: row.get(2)?,
            remediation: row.get(3)?,
            keywords: row.get(4)?,
            created_at: row.get(5)?,
            confidence: row.get::<_, Option<f64>>(6)?.unwrap_or(1.0),
            seen_count: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            repeated_count: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
            status: row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "active".into()),
        })
    }).map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(mem) = r {
            result.push(mem);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn create_memory(
    id: String,
    incident_desc: String,
    code_pattern: String,
    remediation: String,
    keywords: String,
    mem_type: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let mem_type = mem_type.unwrap_or_else(|| "experience".to_string());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO memories (id, incident_desc, code_pattern, remediation, keywords, type, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)",
        params![id, incident_desc, code_pattern, remediation, keywords, mem_type],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_memory(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn distill_session_memory(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<MemorySuggestion, String> {
    let conversation_log = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| e.to_string())?;

        let mut log = String::new();
        let mut count = 0;
        for r in rows {
            if let Ok((role, content)) = r {
                count += 1;
                log.push_str(&format!("{}: {}\n\n", role.to_uppercase(), content));
            }
        }

        if count == 0 {
            return Err("No messages found in this session to distill.".to_string());
        }
        log
    };

    // Fetch upstream LLM credentials
    let active_acc = db.get_active_account().map_err(|e| e.to_string())?;
    let (api_key_raw, api_host, target_model) = if let Some(acc) = active_acc {
        (acc.api_key, acc.api_host, acc.target_model)
    } else {
        let api_key = db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
        let api_host = db.get_setting("api_host").unwrap_or(None).unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let target_model = db.get_setting("target_model").unwrap_or(None).unwrap_or_else(|| "deepseek-chat".to_string());
        (api_key, api_host, target_model)
    };

    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return Err("API Key is not configured. Please configure credentials first.".to_string());
    }
    let api_key = keys[0];

    // Read existing experience memories for merge-aware distillation (FACT.md pattern)
    let existing_experiences = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, incident_desc, code_pattern, remediation, keywords FROM memories WHERE type = 'experience'"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }).map_err(|e| e.to_string())?;

        let mut experiences = Vec::new();
        for r in rows {
            if let Ok((id, desc, pattern, remediation, keywords)) = r {
                experiences.push(format!("[{}] {} | 模式: {} | 修复: {} | 标签: {}", id, desc, pattern, remediation, keywords));
            }
        }
        experiences.join("\n")
    };

    let system_prompt = "You are a Senior Engineering Lead and Code Experience Distillation Engine. \
Your task is to analyze the developer's chat session and distill anti-failure lessons.

IMPORTANT RULES:
1. First check if the NEW lesson overlaps significantly with EXISTING experiences listed below.
2. If it overlaps: merge them into ONE improved entry (update remediation, combine keywords).
3. If it's truly new: create a new entry.
4. Return ALL experiences as a complete JSON array (the merged list), not just the new one.

You must return a JSON array of objects, each with:
- incident_desc: What went wrong
- code_pattern: The risky code pattern or command
- remediation: How to prevent next time
- keywords: Comma-separated tags
- id: The existing ID to update (empty string for new entries)

Return ONLY the raw JSON array, no markdown blocks.";

    let user_prompt = format!(
        "## Existing Experiences:\n{}\n\n## Conversation to Distill:\n{}",
        if existing_experiences.is_empty() { "(none yet)".to_string() } else { existing_experiences },
        conversation_log
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
            "temperature": 0.3
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

    // Parse as array of merged experiences (FACT.md pattern)
    let merged: Vec<serde_json::Value> = serde_json::from_str(clean_json_str)
        .map_err(|e| format!("LLM output did not match expected JSON array: {}. Raw: {}", e, clean_json_str))?;

    let mut saved_count = 0;
    for entry in &merged {
        let id_val = entry["id"].as_str().unwrap_or("");
        let desc = entry["incident_desc"].as_str().unwrap_or("");
        let pattern = entry["code_pattern"].as_str().unwrap_or("");
        let remediation = entry["remediation"].as_str().unwrap_or("");
        let keywords = entry["keywords"].as_str().unwrap_or("");

        if desc.is_empty() { continue; }

        let final_id = if id_val.is_empty() {
            format!("mem_distill_{}_{}", chrono::Utc::now().timestamp_millis(), saved_count)
        } else {
            id_val.to_string()
        };

        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT OR REPLACE INTO memories (id, incident_desc, code_pattern, remediation, keywords, type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'experience', CURRENT_TIMESTAMP)",
            params![final_id, desc, pattern, remediation, keywords],
        );
        saved_count += 1;
    }

    // Return the first entry for UI compatibility
    let first = merged.first().ok_or("LLM returned empty array".to_string())?;
    let result = MemorySuggestion {
        incident_desc: first["incident_desc"].as_str().unwrap_or("").to_string(),
        code_pattern: first["code_pattern"].as_str().unwrap_or("").to_string(),
        remediation: first["remediation"].as_str().unwrap_or("").to_string(),
        keywords: first["keywords"].as_str().unwrap_or("").to_string(),
    };

    Ok(result)
}

/// Automatically distill mistakes from a session and save to memories.
/// Returns None if no mistakes were detected; otherwise returns the suggestion.
/// The memory is saved with source = "auto_distill" for user review.
#[tauri::command]
pub async fn auto_distill_errors(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Option<MemorySuggestion>, String> {
    // 1. Check activity_log for mistake_detected entries in this session
    let mistakes_json = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT details FROM activity_log WHERE action = 'mistake_detected' AND target = ?1 ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| e.to_string())?;

        let mut all = Vec::new();
        for r in rows {
            if let Ok(details) = r {
                all.push(details);
            }
        }
        if all.is_empty() {
            return Ok(None);
        }
        all.join("\n")
    };

    // 2. Check for similar existing memories to avoid duplication
    let existing_keywords: Vec<String> = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT keywords FROM memories")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // 3. Load conversation messages for context
    let conversation_log = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| e.to_string())?;

        let mut log = String::new();
        for r in rows {
            if let Ok((role, content)) = r {
                // Truncate very long messages to keep prompt manageable
                let truncated = if content.len() > 2000 { &content[..2000] } else { &content };
                log.push_str(&format!("{}: {}\n\n", role.to_uppercase(), truncated));
            }
        }
        log
    };

    // 4. Build enhanced prompt with detected mistakes
    let existing_keywords_str = existing_keywords.join(", ");
    let system_prompt = "You are a Senior Engineering Lead and Code Experience Distillation Engine. \
Your task is to analyze the detected development mistakes and the conversation context, then distill a single key anti-failure lesson/pitfall. \
IMPORTANT: Before generating, check if this lesson overlaps significantly with existing memories (keywords listed below). If so, skip and return null. \
You must extract:
1. Incident Description: What went wrong or what mistake was made.
2. Code Pattern: The specific risky code pattern or CLI command that triggered the incident.
3. Remediation: How to resolve and prevent this error next time.
4. Keywords: Comma-separated tag keywords.

Return the response strictly as a JSON object matching this schema:
{
  \"incident_desc\": \"...\",
  \"code_pattern\": \"...\",
  \"remediation\": \"...\",
  \"keywords\": \"tag1,tag2,...\"
}
If the mistake is trivial, already covered by existing memories, or not worth recording, return: {\"skip\": true}
DO NOT wrap the response in markdown blocks.";

    let user_prompt = format!(
        "## Detected Mistakes in This Session:\n{}\n\n## Conversation Context:\n{}\n\n## Existing Memory Keywords (avoid duplication):\n{}\n\nPlease distill a valuable anti-failure lesson from the above mistakes and context.",
        mistakes_json,
        if conversation_log.is_empty() { "(no conversation messages available)".to_string() } else { conversation_log },
        if existing_keywords_str.is_empty() { "(none)".to_string() } else { existing_keywords_str },
    );

    // 5. Fetch upstream LLM credentials
    let active_acc = db.get_active_account().map_err(|e| e.to_string())?;
    let (api_key_raw, api_host, target_model) = if let Some(acc) = active_acc {
        (acc.api_key, acc.api_host, acc.target_model)
    } else {
        let api_key = db.get_setting("api_key").unwrap_or(None).unwrap_or_default();
        let api_host = db.get_setting("api_host").unwrap_or(None).unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let target_model = db.get_setting("target_model").unwrap_or(None).unwrap_or_else(|| "deepseek-chat".to_string());
        (api_key, api_host, target_model)
    };

    let keys: Vec<&str> = api_key_raw.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return Ok(None); // No API key configured, skip silently
    }
    let api_key = keys[0];

    // 6. Send to LLM
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
            "temperature": 0.3
        }))
        .send()
        .await
        .map_err(|e| format!("Auto-distill: failed to connect to LLM: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Ok(None); // Silently skip on upstream error
    }

    let res_json: serde_json::Value = response.json().await
        .map_err(|_| "Auto-distill: failed to parse LLM response".to_string())?;

    let text_content = res_json["choices"][0]["message"]["content"].as_str()
        .ok_or_else(|| "Auto-distill: failed to retrieve content from LLM response".to_string())?;

    let clean_json_str = text_content
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();

    // Check if LLM decided to skip
    if clean_json_str.contains("\"skip\"") && clean_json_str.contains("true") {
        return Ok(None);
    }

    let result: MemorySuggestion = serde_json::from_str(clean_json_str)
        .map_err(|e| format!("Auto-distill: LLM output did not match expected schema: {}. Raw: {}", e, clean_json_str))?;

    // 7. Auto-save to memories with source = "auto_distill"
    let id = format!("mem_auto_{}", chrono::Utc::now().timestamp_millis());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO memories (id, incident_desc, code_pattern, remediation, keywords, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'auto_distill', CURRENT_TIMESTAMP)",
        params![id, result.incident_desc, result.code_pattern, result.remediation, result.keywords],
    ).map_err(|e| e.to_string())?;

    Ok(Some(result))
}
