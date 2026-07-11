use super::*;
use crate::db::DbManager;
use crate::input_validation;
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub active_agent: String,
    pub created_at: String,
}

#[tauri::command]
pub fn get_all_conversations(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ConversationInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    // Exclude archived conversations from the main list — they show in a separate view
    let mut stmt = conn
        .prepare(
            "SELECT id, title, workspace_path, active_agent, created_at
         FROM conversations
         WHERE COALESCE(is_archived, 0) = 0
         ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ConversationInfo {
                id: row.get(0)?,
                title: row.get(1)?,
                workspace_path: row.get(2)?,
                active_agent: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(conv) = r {
            result.push(conv);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn get_archived_conversations(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ConversationInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, workspace_path, active_agent, created_at
         FROM conversations
         WHERE COALESCE(is_archived, 0) = 1
         ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ConversationInfo {
                id: row.get(0)?,
                title: row.get(1)?,
                workspace_path: row.get(2)?,
                active_agent: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(conv) = r {
            result.push(conv);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn archive_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE conversations SET is_archived = 1 WHERE id = ?1",
        params![conversation_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn unarchive_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE conversations SET is_archived = 0 WHERE id = ?1",
        params![conversation_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    /// Runtime enrichment (e.g. image attachment paths); "{}" when absent.
    #[serde(default)]
    pub metadata_json: Option<String>,
}

#[tauri::command]
pub fn get_conversation_messages(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<MessageInfo>, String> {
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, conversation_id, role, content, timestamp, metadata_json FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![conversation_id], |row| {
            Ok(MessageInfo {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                timestamp: row.get(4)?,
                metadata_json: row.get(5).ok(),
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(msg) = r {
            result.push(msg);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn create_conversation(
    id: String,
    title: String,
    workspace_path: String,
    active_agent: String,
    // Set for `/btw` side conversations: the parent whose transcript seeds this
    // branch's first turn. None for normal conversations.
    parent_conversation_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    input_validation::validate_content(&title, "title")?;
    input_validation::validate_workspace_path(&workspace_path, "workspace_path")?;
    if let Some(parent) = parent_conversation_id.as_deref() {
        input_validation::validate_id(parent, "parent_conversation_id")?;
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO conversations (id, title, workspace_path, active_agent, parent_conversation_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, title, workspace_path, active_agent, parent_conversation_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn add_conversation_message(
    id: String,
    conversation_id: String,
    role: String,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    input_validation::validate_content(&content, "content")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO messages (id, conversation_id, role, content) VALUES (?1, ?2, ?3, ?4)",
        params![id, conversation_id, role, content],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let _ = conn.execute(
        "DELETE FROM messages WHERE conversation_id = ?1",
        params![conversation_id],
    );
    conn.execute(
        "DELETE FROM conversations WHERE id = ?1",
        params![conversation_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct DbTask {
    pub id: String,
    pub conversation_id: String,
    pub title: String,
    pub status: String,
    pub order_num: i32,
    pub dependencies: Vec<String>,
}

#[tauri::command]
pub fn get_conversation_tasks(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbTask>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id, conversation_id, title, status, order_num, dependencies FROM tasks WHERE conversation_id = ?1 ORDER BY order_num ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![conversation_id], |row| {
            let deps_str: String = row.get(5)?;
            let dependencies: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
            Ok(DbTask {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                order_num: row.get(4)?,
                dependencies,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(t) = r {
            result.push(t);
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub filename: String,
    pub sender: String,
    pub receiver: String,
    pub command: String,
    pub params: serde_json::Value,
    pub status: String,
    pub timestamp: String,
}

#[tauri::command]
pub fn get_mailbox_messages() -> Result<Vec<MailboxMessage>, String> {
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
    let mut mailbox_dir = home_dir.clone();
    mailbox_dir.push(".omnix");
    mailbox_dir.push("mailbox");

    if !mailbox_dir.exists() {
        return Ok(Vec::new());
    }

    let mut msgs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(mailbox_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&content) {
                        let filename = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let sender = msg["sender"].as_str().unwrap_or("Unknown").to_string();
                        let receiver = msg["receiver"].as_str().unwrap_or("Unknown").to_string();
                        let command = msg["command"].as_str().unwrap_or("Unknown").to_string();
                        let params = msg["params"].clone();
                        let status = msg["status"].as_str().unwrap_or("pending").to_string();
                        let timestamp = msg["timestamp"].as_str().unwrap_or("").to_string();

                        msgs.push(MailboxMessage {
                            filename,
                            sender,
                            receiver,
                            command,
                            params,
                            status,
                            timestamp,
                        });
                    }
                }
            }
        }
    }

    msgs.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(msgs)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccessInfo {
    // Serialized as `ip` / `url` to match the frontend `RemoteAccessInfo` type.
    #[serde(rename = "ip")]
    pub local_ip: String,
    pub port: u16,
    pub token: String,
    #[serde(rename = "url")]
    pub connection_url: String,
}

#[tauri::command]
pub fn get_remote_access_info(db: State<'_, Arc<DbManager>>) -> Result<RemoteAccessInfo, String> {
    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let port_str = db
        .get_setting("proxy_port")
        .unwrap_or(None)
        .unwrap_or_else(|| "1421".to_string());
    let port = port_str.parse::<u16>().unwrap_or(1421);
    let token = db
        .get_setting("remote_token")
        .unwrap_or(None)
        .unwrap_or_default();

    let connection_url = format!("http://{}:{}/remote?token={}", local_ip, port, token);

    Ok(RemoteAccessInfo {
        local_ip,
        port,
        token,
        connection_url,
    })
}

fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}
