use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::db::DbManager;
use crate::input_validation;

use super::skills::create_skill_core;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationCandidate {
    pub id: String,
    pub conversation_id: String,
    pub workspace_path: String,
    pub candidate_type: String,
    pub title: String,
    pub summary: String,
    pub payload_json: String,
    pub evidence_json: String,
    pub model_id: String,
    pub status: String,
    pub created_at: String,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeneratedCandidate {
    #[serde(rename = "type")]
    candidate_type: String,
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    payload: Value,
    #[serde(default)]
    evidence_message_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GeneratedCandidates {
    candidates: Vec<GeneratedCandidate>,
}

fn make_id(prefix: &str) -> String {
    format!(
        "{prefix}_{}_{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}

fn clean_json(raw: &str) -> &str {
    raw.trim()
        .strip_prefix("```json")
        .unwrap_or(raw.trim())
        .trim_end_matches("```")
        .trim()
}

fn parse_candidates(raw: &str) -> Result<Vec<GeneratedCandidate>, String> {
    let parsed: GeneratedCandidates = serde_json::from_str(clean_json(raw))
        .map_err(|error| format!("蒸馏模型返回的 JSON 无法解析：{error}"))?;
    let candidates = parsed
        .candidates
        .into_iter()
        .filter(|candidate| {
            matches!(
                candidate.candidate_type.as_str(),
                "memory" | "skill" | "protocol"
            )
        })
        .filter(|candidate| !candidate.title.trim().is_empty())
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err("模型没有生成可审阅的蒸馏候选".into());
    }
    Ok(candidates)
}

fn get_candidate(db: &DbManager, candidate_id: &str) -> Result<DistillationCandidate, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id, conversation_id, workspace_path, candidate_type, title, summary,
                payload_json, evidence_json, model_id, status, created_at, reviewed_at
         FROM distillation_inbox WHERE id = ?1",
        params![candidate_id],
        |row| {
            Ok(DistillationCandidate {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                workspace_path: row.get(2)?,
                candidate_type: row.get(3)?,
                title: row.get(4)?,
                summary: row.get(5)?,
                payload_json: row.get(6)?,
                evidence_json: row.get(7)?,
                model_id: row.get(8)?,
                status: row.get(9)?,
                created_at: row.get(10)?,
                reviewed_at: row.get(11)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn distill_conversation_to_inbox(
    conversation_id: String,
    model_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DistillationCandidate>, String> {
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    if model_id.trim().is_empty() {
        return Err("请先选择用于蒸馏的模型".into());
    }
    crate::knowledge::resolve_chat_platform(&db, &model_id)?;

    let (workspace_path, transcript) = {
        let conn = db.get_connection().map_err(|error| error.to_string())?;
        let workspace_path = conn
            .query_row(
                "SELECT workspace_path FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "会话不存在".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, role, content FROM messages
                 WHERE conversation_id = ?1 AND content <> ''
                 ORDER BY sequence ASC, timestamp ASC LIMIT 300",
            )
            .map_err(|error| error.to_string())?;
        let lines = stmt
            .query_map(params![conversation_id], |row| {
                Ok(format!(
                    "[{}] {}: {}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?
                ))
            })
            .map_err(|error| error.to_string())?
            .flatten()
            .collect::<Vec<_>>();
        (workspace_path, lines.join("\n\n"))
    };
    if transcript.trim().is_empty() {
        return Err("该会话没有可蒸馏的真实消息".into());
    }

    let system = r#"你是 OMNIX Workbench 的开发经验蒸馏器。只从给定会话中提取有证据支持、可复用的候选，不得臆造。
返回严格 JSON：{"candidates":[{"type":"memory|skill|protocol","title":"...","summary":"...","payload":{},"evidence_message_ids":["..."]}]}。
memory payload 必须包含 incident_desc、code_pattern、remediation、keywords；skill payload 必须包含 name、description、content；protocol payload 必须包含 target_file、proposed_change、rationale。没有足够证据时返回较少候选。"#;
    let proxy_port = db
        .get_setting("proxy_port")
        .ok()
        .flatten()
        .unwrap_or_else(|| "1421".into());
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|error| error.to_string())?
        .post(format!("http://127.0.0.1:{proxy_port}/v1/chat/completions"))
        .json(&serde_json::json!({
            "model": model_id,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": transcript}
            ],
            "temperature": 0.2
        }))
        .send()
        .await
        .map_err(|error| format!("无法连接 OMNIX 模型网关：{error}"))?;
    let status = response.status();
    let response_json: Value = response.json().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("蒸馏请求失败（{status}）：{response_json}"));
    }
    let raw = response_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "蒸馏模型没有返回文本内容".to_string())?;
    let generated = parse_candidates(raw)?;

    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut saved = Vec::new();
    for candidate in generated {
        let id = make_id("distill_candidate");
        let payload_json = candidate.payload.to_string();
        let evidence_json = serde_json::to_string(&candidate.evidence_message_ids)
            .map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO distillation_inbox
             (id, conversation_id, workspace_path, candidate_type, title, summary,
              payload_json, evidence_json, model_id, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending')",
            params![
                id,
                conversation_id,
                workspace_path,
                candidate.candidate_type,
                candidate.title,
                candidate.summary,
                payload_json,
                evidence_json,
                model_id,
            ],
        )
        .map_err(|error| error.to_string())?;
        saved.push(get_candidate(&db, &id)?);
    }
    Ok(saved)
}

#[tauri::command]
pub fn list_distillation_inbox(
    status: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DistillationCandidate>, String> {
    let status = status.unwrap_or_else(|| "pending".into());
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id FROM distillation_inbox
             WHERE ?1 = 'all' OR status = ?1 ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let ids = stmt
        .query_map(params![status], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .flatten()
        .collect::<Vec<_>>();
    ids.into_iter().map(|id| get_candidate(&db, &id)).collect()
}

#[tauri::command]
pub fn review_distillation_candidate(
    candidate_id: String,
    approved: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<DistillationCandidate, String> {
    input_validation::validate_id(&candidate_id, "candidate_id")?;
    let candidate = get_candidate(&db, &candidate_id)?;
    if candidate.status != "pending" {
        return Err("该蒸馏候选已经处理".into());
    }
    let payload: Value = serde_json::from_str(&candidate.payload_json)
        .map_err(|error| format!("候选内容损坏：{error}"))?;
    if approved {
        match candidate.candidate_type.as_str() {
            "memory" => {
                let field = |name: &str| payload[name].as_str().unwrap_or("").trim().to_string();
                let incident_desc = field("incident_desc");
                let remediation = field("remediation");
                if incident_desc.is_empty() || remediation.is_empty() {
                    return Err("记忆候选缺少事件或修复方案".into());
                }
                let conn = db.get_connection().map_err(|error| error.to_string())?;
                conn.execute(
                    "INSERT INTO memories
                     (id, incident_desc, code_pattern, remediation, keywords, type, source,
                      workspace_path, evidence_json, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'experience', 'distillation_inbox', ?6, ?7, 'active')",
                    params![
                        make_id("memory"), incident_desc, field("code_pattern"), remediation,
                        field("keywords"), candidate.workspace_path, candidate.evidence_json
                    ],
                )
                .map_err(|error| error.to_string())?;
            }
            "skill" => {
                let name = payload["name"].as_str().unwrap_or("").trim();
                let description = payload["description"].as_str().unwrap_or("").trim();
                let content = payload["content"].as_str().unwrap_or("").trim();
                if name.is_empty() || content.is_empty() {
                    return Err("技能候选缺少名称或内容".into());
                }
                create_skill_core(&db, name, description, "Core", &[], content, false)?;
            }
            "protocol" => {
                let conn = db.get_connection().map_err(|error| error.to_string())?;
                conn.execute(
                    "INSERT INTO evolution_proposals
                     (id, workspace_path, proposal_type, title, rationale, diff_json, status, applied_at)
                     VALUES (?1, ?2, 'protocol_update', ?3, ?4, ?5, 'approved', datetime('now'))",
                    params![
                        make_id("evolution"), candidate.workspace_path, candidate.title,
                        payload["rationale"].as_str().unwrap_or(&candidate.summary), candidate.payload_json
                    ],
                )
                .map_err(|error| error.to_string())?;
            }
            _ => return Err("不支持的蒸馏候选类型".into()),
        }
    }
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE distillation_inbox SET status = ?1, reviewed_at = datetime('now') WHERE id = ?2",
        params![if approved { "approved" } else { "rejected" }, candidate_id],
    )
    .map_err(|error| error.to_string())?;
    get_candidate(&db, &candidate_id)
}

#[cfg(test)]
mod tests {
    use super::parse_candidates;

    #[test]
    fn parses_only_supported_evidence_candidates() {
        let raw = r#"{"candidates":[
          {"type":"memory","title":"Avoid lock across await","payload":{"incident_desc":"deadlock"},"evidence_message_ids":["m1"]},
          {"type":"unknown","title":"ignored","payload":{}}
        ]}"#;
        let parsed = parse_candidates(raw).expect("valid candidate payload");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].candidate_type, "memory");
        assert_eq!(parsed[0].evidence_message_ids, vec!["m1"]);
    }
}
