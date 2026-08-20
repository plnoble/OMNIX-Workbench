use super::*;
use crate::db::DbManager;
use crate::input_validation;
use rusqlite::params;
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

/// 单会话的消息（不分页的全量版，仍用于打开会话时的一次性加载之外的场景）。
/// 靠 `idx_messages_conversation_timestamp` 复合索引直接出有序结果，
/// 不再 `USE TEMP B-TREE FOR ORDER BY`（那是把整个会话排一遍）。
/// 会话列表只取最近 N 条。
///
/// **不做无限滚动**：侧栏是按 agent 分组渲染的，分页会打断分组语义——翻到第二页
/// 可能整组消失。所以是「最近 N + 搜索」，而搜索走 SQL（见 `SEARCH_CONVERSATIONS_SQL`），
/// 不是前端过滤——前端过滤需要全量在手，等于没分页。
pub(crate) const LIST_CONVERSATIONS_PAGE_SQL: &str = "SELECT id, title, workspace_path, active_agent, created_at
     FROM conversations
     WHERE is_archived = ?1
     ORDER BY created_at DESC
     LIMIT ?2";

pub(crate) const COUNT_CONVERSATIONS_SQL: &str =
    "SELECT COUNT(*) FROM conversations WHERE is_archived = ?1";

/// 标题搜索。`LIKE` 前面带通配符用不上索引，但搜索是用户主动触发的低频操作，
/// 而且结果有上限——比「把全部会话拉到前端再 filter」好得多。
pub(crate) const SEARCH_CONVERSATIONS_SQL: &str = "SELECT id, title, workspace_path, active_agent, created_at
     FROM conversations
     WHERE is_archived = ?1 AND title LIKE ?2 ESCAPE '\\'
     ORDER BY created_at DESC
     LIMIT ?3";

pub(crate) const CONVERSATION_MESSAGES_SQL: &str = "SELECT id, conversation_id, role, content, timestamp, metadata_json
     FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC, rowid ASC";

/// 增量拉取：只取排在 `(?2, ?3)` 之后的消息。
///
/// 次序键是 `(timestamp, rowid)` 而不是 timestamp 单独。`CURRENT_TIMESTAMP` 是
/// **秒级**的，同一秒内落库的消息时间戳完全相同——实测库里 20 条消息就有 6 组并列。
/// 只按 timestamp 做游标，同秒的那些要么被跳过、要么被重复拉。
///
/// 也不能用 `sequence` 列：它是按 **session** 算的，同一个会话跨多次会话时会重号
/// （实测 18 条消息的会话只有 17 个 distinct sequence）。
///
/// `rowid` 对只追加的表是天然的总序，而且它正是 `idx_messages_conversation_timestamp`
/// 里的隐式次序键——所以这条查询用的是同一个索引，不需要额外排序。
pub(crate) const MESSAGES_SINCE_SQL: &str = "SELECT id, conversation_id, role, content, timestamp, metadata_json
     FROM messages
     WHERE conversation_id = ?1
       AND (timestamp > ?2 OR (timestamp = ?2 AND rowid > ?3))
     ORDER BY timestamp ASC, rowid ASC";


/// 一页会话 + 总数。
///
/// `total` 是给界面写「显示最近 100 个 / 共 1,240 个」用的。只截断不告诉用户
/// 总数，和消息那边静默截断是同一个毛病。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ConversationPage {
    pub conversations: Vec<ConversationInfo>,
    pub total: i64,
}

fn read_conversations(
    stmt: &mut rusqlite::Statement<'_>,
    params: impl rusqlite::Params,
) -> Result<Vec<ConversationInfo>, String> {
    let rows = stmt
        .query_map(params, |row| {
            Ok(ConversationInfo {
                id: row.get(0)?,
                title: row.get(1)?,
                workspace_path: row.get(2)?,
                active_agent: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

pub(crate) fn list_conversations_page_core(
    db: &DbManager,
    archived: bool,
    limit: u32,
) -> Result<ConversationPage, String> {
    let limit = limit.clamp(1, 1000);
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let flag = i32::from(archived);
    let mut stmt = conn
        .prepare(LIST_CONVERSATIONS_PAGE_SQL)
        .map_err(|e| e.to_string())?;
    let conversations = read_conversations(&mut stmt, params![flag, limit])?;
    let total: i64 = conn
        .query_row(COUNT_CONVERSATIONS_SQL, params![flag], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(ConversationPage { conversations, total })
}

/// 按标题搜索。`%` `_` 要转义，否则用户输入的下划线会变成通配符。
pub(crate) fn search_conversations_core(
    db: &DbManager,
    query: &str,
    archived: bool,
    limit: u32,
) -> Result<Vec<ConversationInfo>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 1000);
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(SEARCH_CONVERSATIONS_SQL)
        .map_err(|e| e.to_string())?;
    read_conversations(
        &mut stmt,
        params![i32::from(archived), format!("%{escaped}%"), limit],
    )
}

#[tauri::command]
pub fn search_conversations(
    query: String,
    archived: bool,
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ConversationInfo>, String> {
    search_conversations_core(&db, &query, archived, limit)
}

#[tauri::command]
pub fn get_all_conversations(
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<ConversationPage, String> {
    // 归档的走单独的视图，不进主列表。
    list_conversations_page_core(&db, false, limit)
}

#[tauri::command]
pub fn get_archived_conversations(
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<ConversationPage, String> {
    list_conversations_page_core(&db, true, limit)
}

pub(crate) fn set_conversation_archived_core(
    db: &DbManager,
    conversation_id: &str,
    archived: bool,
) -> Result<(), String> {
    input_validation::validate_id(conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE conversations SET is_archived = ?1 WHERE id = ?2",
        params![if archived { 1 } else { 0 }, conversation_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn archive_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    set_conversation_archived_core(&db, &conversation_id, true)
}

#[tauri::command]
pub fn unarchive_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    set_conversation_archived_core(&db, &conversation_id, false)
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
    get_conversation_messages_core(&db, &conversation_id)
}

/// 消息分页：**最近 N 条**，往回翻。
///
/// 聊天的语义是从最新往回看，不是从最早往后翻，所以初次加载取尾部。
/// 倒着取再反转，靠的是 SQLite 能反向扫索引——`ORDER BY timestamp DESC, rowid DESC`
/// 用的还是 `idx_messages_conversation_timestamp`，不产生临时排序。
pub(crate) const MESSAGES_TAIL_SQL: &str = "SELECT id, conversation_id, role, content, timestamp, metadata_json
     FROM messages WHERE conversation_id = ?1
     ORDER BY timestamp DESC, rowid DESC LIMIT ?2";

/// 往回翻一页：取排在 `(?2, ?3)` **之前**的最后 N 条。
pub(crate) const MESSAGES_BEFORE_SQL: &str = "SELECT id, conversation_id, role, content, timestamp, metadata_json
     FROM messages
     WHERE conversation_id = ?1
       AND (timestamp < ?2 OR (timestamp = ?2 AND rowid < ?3))
     ORDER BY timestamp DESC, rowid DESC LIMIT ?4";

/// 某条消息**之前**还剩多少条。界面要把这个数说出来。
pub(crate) const OLDER_COUNT_SQL: &str = "SELECT COUNT(*) FROM messages
     WHERE conversation_id = ?1
       AND (timestamp < ?2 OR (timestamp = ?2 AND rowid < ?3))";

/// 一页消息。
///
/// `older_remaining` 不是可选的装饰——它就是界面上那句「上面还有 X 条」。
/// 只加 LIMIT 不显示剩余数，用户会以为历史丢了，那比慢更糟；这是这一版分页
/// 的硬约束（见 `docs/分页方案-W18.md`）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct MessagePage {
    /// 升序（老 → 新），可以直接接在界面顶部。
    pub messages: Vec<MessageInfo>,
    /// 这一页**之上**还有多少条没加载。0 表示到头了。
    pub older_remaining: i64,
}

/// 增量拉取的结果。
///
/// `is_full` 不是装饰：游标失效（比如那条消息已被压缩删掉）时后端会退回全量，
/// 前端必须知道这次该**替换**还是**追加**——分不清就会重复渲染一整段历史。
/// 这类「悄悄换了语义」正是这个项目反复在修的毛病，所以让它出现在类型里。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct MessagesDelta {
    pub messages: Vec<MessageInfo>,
    pub is_full: bool,
}

fn read_messages(
    stmt: &mut rusqlite::Statement<'_>,
    params: impl rusqlite::Params,
) -> Result<Vec<MessageInfo>, String> {
    let rows = stmt
        .query_map(params, |row| {
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
    Ok(rows.flatten().collect())
}

/// 取一页消息：不给游标就是最近 N 条，给了就是那条**之前**的 N 条。
///
/// 返回的 `messages` 是升序的（老 → 新），前端直接往顶上接即可。
pub(crate) fn get_messages_page_core(
    db: &DbManager,
    conversation_id: &str,
    before_message_id: Option<&str>,
    limit: u32,
) -> Result<MessagePage, String> {
    input_validation::validate_id(conversation_id, "conversation_id")?;
    // 上限兜底：limit 来自前端，别让一个笔误把整张表拉出来——那正是这次要消掉的
    // 行为。下限 1，否则一页零条会让「加载更早」永远点不动。
    let limit = limit.clamp(1, 500);
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // 游标失效（那条已被压缩删掉等）就退回尾部。这里退回是安全的：用户看到的仍是
    // 最新的一页，而不是一段接不上的历史。
    let cursor: Option<(String, i64)> = match before_message_id {
        Some(id) if !id.is_empty() => conn
            .query_row(
                "SELECT timestamp, rowid FROM messages WHERE id = ?1 AND conversation_id = ?2",
                params![id, conversation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok(),
        _ => None,
    };

    let mut descending = match &cursor {
        Some((ts, rowid)) => {
            let mut stmt = conn.prepare(MESSAGES_BEFORE_SQL).map_err(|e| e.to_string())?;
            read_messages(&mut stmt, params![conversation_id, ts, rowid, limit])?
        }
        None => {
            let mut stmt = conn.prepare(MESSAGES_TAIL_SQL).map_err(|e| e.to_string())?;
            read_messages(&mut stmt, params![conversation_id, limit])?
        }
    };
    // 查询是倒着取的（为了拿到「最后 N 条」），返回给界面要正过来。
    descending.reverse();
    let messages = descending;

    // 这一页最老的一条之前还剩多少。空页就是 0——没有更老的了。
    let older_remaining: i64 = match messages.first() {
        Some(first) => {
            let (ts, rowid): (String, i64) = conn
                .query_row(
                    "SELECT timestamp, rowid FROM messages WHERE id = ?1",
                    params![first.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            conn.query_row(
                OLDER_COUNT_SQL,
                params![conversation_id, ts, rowid],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?
        }
        None => 0,
    };

    Ok(MessagePage { messages, older_remaining })
}

#[tauri::command]
pub fn get_messages_page(
    conversation_id: String,
    before_message_id: Option<String>,
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<MessagePage, String> {
    get_messages_page_core(&db, &conversation_id, before_message_id.as_deref(), limit)
}

/// 只取「前端还没有的那几条」。
///
/// 原来每收到一个 agent 事件就把整个会话重新拉一遍并整体替换（一轮对话几十个
/// 事件），会话一长就是每个事件一次全量读 + 全量重渲染。
///
/// `after_message_id` 传前端手上最后一条的 id；找不到那条（已被压缩删除等）就
/// 退回全量，并把 `is_full` 置真——**不静默降级**。
pub(crate) fn get_messages_since_core(
    db: &DbManager,
    conversation_id: &str,
    after_message_id: Option<&str>,
) -> Result<MessagesDelta, String> {
    input_validation::validate_id(conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    // 游标是那条消息在 `(timestamp, rowid)` 上的位置。按主键查，代价可忽略。
    let cursor: Option<(String, i64)> = match after_message_id {
        Some(id) if !id.is_empty() => conn
            .query_row(
                "SELECT timestamp, rowid FROM messages WHERE id = ?1 AND conversation_id = ?2",
                params![id, conversation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok(),
        _ => None,
    };

    match cursor {
        Some((ts, rowid)) => {
            let mut stmt = conn.prepare(MESSAGES_SINCE_SQL).map_err(|e| e.to_string())?;
            let messages = read_messages(&mut stmt, params![conversation_id, ts, rowid])?;
            Ok(MessagesDelta { messages, is_full: false })
        }
        None => {
            let mut stmt = conn
                .prepare(CONVERSATION_MESSAGES_SQL)
                .map_err(|e| e.to_string())?;
            let messages = read_messages(&mut stmt, params![conversation_id])?;
            Ok(MessagesDelta { messages, is_full: true })
        }
    }
}

#[tauri::command]
pub fn get_messages_since(
    conversation_id: String,
    after_message_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<MessagesDelta, String> {
    get_messages_since_core(&db, &conversation_id, after_message_id.as_deref())
}

pub(crate) fn get_conversation_messages_core(
    db: &DbManager,
    conversation_id: &str,
) -> Result<Vec<MessageInfo>, String> {
    input_validation::validate_id(conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(CONVERSATION_MESSAGES_SQL)
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
    for msg in rows.flatten() {
        result.push(msg);
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
    create_conversation_core(
        &db,
        &id,
        &title,
        &workspace_path,
        &active_agent,
        parent_conversation_id.as_deref(),
    )
}

pub(crate) fn create_conversation_core(
    db: &DbManager,
    id: &str,
    title: &str,
    workspace_path: &str,
    active_agent: &str,
    parent_conversation_id: Option<&str>,
) -> Result<(), String> {
    input_validation::validate_id(id, "id")?;
    input_validation::validate_content(title, "title")?;
    input_validation::validate_workspace_path(workspace_path, "workspace_path")?;
    if let Some(parent) = parent_conversation_id {
        input_validation::validate_id(parent, "parent_conversation_id")?;
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO conversations (id, title, workspace_path, active_agent, parent_conversation_id) VALUES (?1, ?2, ?3, ?4, ?5)",
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
    add_conversation_message_core(&db, &id, &conversation_id, &role, &content)
}

pub(crate) fn add_conversation_message_core(
    db: &DbManager,
    id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> Result<(), String> {
    input_validation::validate_id(id, "id")?;
    input_validation::validate_id(conversation_id, "conversation_id")?;
    input_validation::validate_content(content, "content")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO messages (id, conversation_id, role, content) VALUES (?1, ?2, ?3, ?4)",
        params![id, conversation_id, role, content],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// 删一条对话时要跟着删掉的从属表。
///
/// 全部显式删，**不依赖 `ON DELETE CASCADE`**，有两个原因：
/// 一是 `tasks` 和 `distillation_inbox` 压根没声明外键，级联救不了它们——以前
/// 只手删了 `messages`，这两张表的行会永远留在库里指向一个不存在的会话；
/// 二是声明了外键的那几张表用的是 `CREATE TABLE IF NOT EXISTS`，老库里的表结构
/// 是当年建的那一版，未必带着今天写在 schema 里的这条外键。显式删一遍两种情况
/// 都覆盖住，代价只是一条走索引的 DELETE。
///
/// `autopilot_runs` 带 `conversation_id` 但**特意不在这里**：它的生命周期跟着
/// autopilot 走（删 autopilot 时才清），是那条自动任务的一条历史记录，不该因为
/// 用户删掉一次会话就少一行。
pub(crate) const CONVERSATION_OWNED_TABLES: &[&str] = &[
    "messages",
    "tasks",
    "chat_knowledge_bindings",
    "distillation_inbox",
    "agent_sessions",
    "conversation_goals",
];

#[tauri::command]
pub fn delete_conversation(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    delete_conversation_core(&db, &conversation_id)
}

pub(crate) fn delete_conversation_core(
    db: &DbManager,
    conversation_id: &str,
) -> Result<(), String> {
    input_validation::validate_id(conversation_id, "conversation_id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    for table in CONVERSATION_OWNED_TABLES {
        // 表名来自上面那个常量数组，不是调用方传的，拼进 SQL 是安全的。
        conn.execute(
            &format!("DELETE FROM {table} WHERE conversation_id = ?1"),
            params![conversation_id],
        )
        .map_err(|e| format!("清理 {table} 失败: {e}"))?;
    }
    conn.execute(
        "DELETE FROM conversations WHERE id = ?1",
        params![conversation_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
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
    /// 一次性配对码的有效期（秒）。前端拿它做倒计时并在到期前自动换一个。
    pub code_ttl_secs: i64,
}

/// 手机配对信息。**每调用一次就发一个新的一次性配对码**——URL 里那段凭据
/// 5 分钟过期、扫一次即废，不再是那个泄一次就永久有效的 `remote_token`。
///
/// `token` 仍然返回：它是 `x-omnix-remote-token` 头的值，脚本/调试要拿它直连
/// 网关。头不进浏览器历史，所以留在头里是安全的。
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

    let code = crate::remote_session::mint_code(chrono::Utc::now().timestamp())?;
    let connection_url = format!("http://{}:{}/remote?code={}", local_ip, port, code);

    Ok(RemoteAccessInfo {
        local_ip,
        port,
        token,
        connection_url,
        code_ttl_secs: crate::remote_session::code_ttl_secs(),
    })
}

fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbManager;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_conv_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    fn ws() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    fn seed(db: &DbManager, id: &str) {
        create_conversation_core(db, id, "标题", &ws(), "Claude Code", None).unwrap();
    }

    /// `INSERT OR REPLACE` deletes the old row first, which CASCADE-wipes
    /// messages. A repeated create with the same id must leave history alone.
    #[test]
    fn creating_the_same_conversation_twice_does_not_wipe_messages() {
        let db = test_db("replace");
        seed(&db, "c1");
        add_conversation_message_core(&db, "m1", "c1", "user", "第一句").unwrap();
        create_conversation_core(&db, "c1", "新标题", &ws(), "Claude Code", None).unwrap();
        let count: i64 = db
            .get_connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = 'c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "同 id 再创建一次把消息级联删了");
    }

    fn ids(list: &[ConversationInfo]) -> Vec<&str> {
        list.iter().map(|c| c.id.as_str()).collect()
    }

    /// 归档过的不能出现在主列表里，也不能在归档列表里漏掉。
    ///
    /// 两个列表以前是两份逐字重复的函数，现在共用一条 SQL，参数取反——这条测试
    /// 同时钉住「取反没写反」。
    #[test]
    fn archiving_moves_a_conversation_between_the_two_lists() {
        let db = test_db("archive");
        seed(&db, "c_kept");
        seed(&db, "c_archived");
        set_conversation_archived_core(&db, "c_archived", true).unwrap();

        let main = list_conversations_page_core(&db, false, 100).unwrap().conversations;
        let archived = list_conversations_page_core(&db, true, 100).unwrap().conversations;
        assert_eq!(ids(&main), vec!["c_kept"], "归档的还留在主列表里");
        assert_eq!(ids(&archived), vec!["c_archived"]);

        set_conversation_archived_core(&db, "c_archived", false).unwrap();
        let main = list_conversations_page_core(&db, false, 100).unwrap().conversations;
        assert_eq!(main.len(), 2, "取消归档后要回到主列表");
        assert!(list_conversations_page_core(&db, true, 100).unwrap().conversations.is_empty());
    }

    /// 外键必须是开的，而且级联要真的连着跑。
    ///
    /// schema 里十几处 `ON DELETE CASCADE` 全指望它，而 SQLite 的编译期默认是
    /// **关**的——以前能生效纯粹是因为 `libsqlite3-sys` 的 bundled 构建带了
    /// `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`，一个依赖的开关。
    ///
    /// 这里不只读 pragma，还走一条真实的两级级联：`runtime_events` 是全库最大的
    /// 增长源，它没有 `conversation_id`，只能靠 `agent_sessions` 那条外键被带走。
    /// 外键一旦关掉，删会话就会把它整段留在库里——而且没有任何报错。
    #[test]
    fn deleting_a_conversation_cascades_down_to_runtime_events() {
        let db = test_db("pragma");
        {
            let conn = db.get_connection().unwrap();
            let on: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(on, 1, "外键没开，schema 里所有 ON DELETE CASCADE 都是装饰");
        }
        seed(&db, "c1");
        {
            let conn = db.get_connection().unwrap();
            conn.execute(
                "INSERT INTO agent_sessions (id, conversation_id, agent_id, adapter_kind,
                    executable_path, workspace_path, model_json, permission_json, work_mode)
                 VALUES ('s1', 'c1', 'a', 'acp', 'x', 'y', '{}', '{}', 'chat')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO runtime_events (id, session_id, sequence, kind)
                 VALUES ('e1', 's1', 1, 'raw_log')",
                [],
            )
            .unwrap();
        }

        delete_conversation_core(&db, "c1").unwrap();

        let left: i64 = db
            .get_connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM runtime_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "会话删了，它的运行事件还留在库里");
    }

    /// 删对话必须把从属数据一起带走。
    ///
    /// `tasks` 和 `distillation_inbox` 根本没声明外键，级联救不了；而以前删除
    /// 只手删了 `messages`，所以这两张表的行会永远留在库里指向一个不存在的会话。
    #[test]
    fn deleting_a_conversation_takes_its_dependent_rows_with_it() {
        let db = test_db("delete");
        seed(&db, "c1");
        seed(&db, "c2");
        add_conversation_message_core(&db, "m1", "c1", "user", "留下的痕迹").unwrap();
        add_conversation_message_core(&db, "m2", "c2", "user", "别人的消息").unwrap();
        {
            let conn = db.get_connection().unwrap();
            conn.execute(
                "INSERT INTO knowledge_bases (id, name) VALUES ('kb1', '库')",
                [],
            )
            .unwrap();
            for cid in ["c1", "c2"] {
                conn.execute(
                    "INSERT INTO tasks (id, conversation_id, title, status, order_num)
                     VALUES (?1, ?2, '任务', 'todo', 0)",
                    params![format!("t_{cid}"), cid],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO chat_knowledge_bindings (conversation_id, knowledge_base_id)
                     VALUES (?1, 'kb1')",
                    params![cid],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO distillation_inbox (id, conversation_id, candidate_type, title, model_id)
                     VALUES (?1, ?2, 'experience', '候选', 'm')",
                    params![format!("d_{cid}"), cid],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO conversation_goals (conversation_id, objective) VALUES (?1, '目标')",
                    params![cid],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO agent_sessions (id, conversation_id, agent_id, adapter_kind,
                        executable_path, workspace_path, model_json, permission_json, work_mode)
                     VALUES (?1, ?2, 'a', 'acp', 'x', 'y', '{}', '{}', 'chat')",
                    params![format!("s_{cid}"), cid],
                )
                .unwrap();
            }
        }

        delete_conversation_core(&db, "c1").unwrap();

        // 这份清单是**手写死的**，不能复用 `CONVERSATION_OWNED_TABLES`：那样从
        // 常量里删掉一张表，测试就跟着不再检查它——反向验证照样是绿的（试过）。
        let must_be_gone = [
            "messages",
            "tasks",
            "chat_knowledge_bindings",
            "distillation_inbox",
            "agent_sessions",
            "conversation_goals",
        ];
        let conn = db.get_connection().unwrap();
        for table in must_be_gone {
            let left: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE conversation_id = 'c1'"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(left, 0, "{table} 里还留着被删会话的行");
            let others: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE conversation_id = 'c2'"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(others, 1, "{table} 误删了别的会话的行");
        }
        assert_eq!(ids(&list_conversations_page_core(&db, false, 100).unwrap().conversations), vec!["c2"]);
    }

    /// 接线守卫：schema 里每张带 `conversation_id` 的表，都必须在
    /// `CONVERSATION_OWNED_TABLES` 里，或者在下面的豁免名单里写清楚为什么不删。
    ///
    /// 新加一张带 `conversation_id` 的表却忘了决定它的归属时，这条会红。
    #[test]
    fn every_table_with_a_conversation_id_has_a_documented_delete_policy() {
        // 豁免：生命周期跟着 autopilot 走，删 autopilot 时才清。会话被删后这行
        // 仍是那条自动任务的一条历史记录。
        const NOT_OWNED: &[&str] = &["autopilot_runs"];

        let db = test_db("guard");
        let conn = db.get_connection().unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .flatten()
            .collect();

        let mut undecided = Vec::new();
        for table in tables {
            let has_col = conn
                .prepare(&format!("SELECT conversation_id FROM {table} LIMIT 0"))
                .is_ok();
            if !has_col {
                continue;
            }
            let owned = CONVERSATION_OWNED_TABLES.contains(&table.as_str());
            let exempt = NOT_OWNED.contains(&table.as_str());
            // 两边都在是自相矛盾的，而先查 owned 再查 exempt 会把它悄悄咽掉。
            assert!(!(owned && exempt), "{table} 同时被标成「要删」和「不删」");
            if !owned && !exempt {
                undecided.push(table);
            }
        }
        assert!(
            undecided.is_empty(),
            "这些表带 conversation_id，但没人决定删会话时要不要清它们：{undecided:?}"
        );
    }

    /// 消息按时间正序返回，且只返回本会话的。
    #[test]
    fn messages_are_scoped_to_their_conversation() {
        let db = test_db("messages");
        seed(&db, "c1");
        seed(&db, "c2");
        add_conversation_message_core(&db, "m1", "c1", "user", "第一句").unwrap();
        add_conversation_message_core(&db, "m2", "c2", "user", "别的会话").unwrap();

        let msgs = get_conversation_messages_core(&db, "c1").unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "第一句");
        assert_eq!(msgs[0].conversation_id, "c1");
    }

    /// 空 id 不能一路走到 SQL——这一批读写命令都验，`get_conversation_tasks`
    /// 以前是唯一漏掉的那个。
    #[test]
    fn blank_ids_are_rejected_before_touching_sql() {
        let db = test_db("validate");
        assert!(get_conversation_messages_core(&db, "").is_err());
        assert!(delete_conversation_core(&db, "").is_err());
        assert!(set_conversation_archived_core(&db, "", true).is_err());
        assert!(create_conversation_core(&db, "", "t", &ws(), "a", None).is_err());
        // 父会话 id 也要验：`/btw` 分支会把它带进来。
        assert!(create_conversation_core(&db, "c", "t", &ws(), "a", Some("")).is_err());
    }

}

/// 查询计划守卫：会话列表与消息列表不许退回全表扫描或临时排序。
///
/// 这两条是应用里跑得最勤的查询——每开一个会话、每收一个 agent 事件都会打一次。
/// 它们原来的计划是：
///
/// ```text
/// messages:      SEARCH USING INDEX idx_messages_conversation_id (conversation_id=?)
///                USE TEMP B-TREE FOR ORDER BY        ← 把整个会话排一遍
/// conversations: SCAN conversations                   ← 全表扫
///                USE TEMP B-TREE FOR ORDER BY
/// ```
///
/// 现在靠 `(conversation_id, timestamp)` 与 `(is_archived, created_at)` 两条复合
/// 索引直接出有序结果。**这是将来做分页的前提**：没有它，加了 LIMIT 也只省传输量，
/// 扫描和排序一点没少（见 `docs/分页方案-W18.md` 阶段 0）。
///
/// 索引被删掉、或者查询被改回不可 sarg 的写法（比如给条件列包一层 `COALESCE`），
/// 这条会红。
#[cfg(test)]
mod query_plan_tests {
    use super::*;
    use crate::db::DbManager;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_qplan_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    /// 查询计划不依赖参数的**取值**，但每个 `?N` 都得绑上东西，否则执行时报
    /// `InvalidParameterCount`。所以按 SQL 里出现的最大占位符号数补齐。
    fn plan(db: &DbManager, sql: &str) -> String {
        let n = (1..=9)
            .filter(|i| sql.contains(&format!("?{i}")))
            .count();
        let binds: Vec<String> = (0..n).map(|_| "0".to_string()).collect();
        let conn = db.get_connection().unwrap();
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let rows: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(binds), |r| r.get::<_, String>(3))
            .unwrap()
            .flatten()
            .collect();
        rows.join(" | ")
    }

    #[test]
    fn messages_come_out_sorted_by_the_index_not_a_temp_btree() {
        let db = test_db("msg");
        let p = plan(&db, CONVERSATION_MESSAGES_SQL);
        assert!(
            p.contains("idx_messages_conversation_timestamp"),
            "没走复合索引，实际计划：{p}"
        );
        assert!(
            !p.contains("TEMP B-TREE"),
            "还在临时排序（等于把整个会话排一遍）：{p}"
        );
    }

    #[test]
    fn the_conversation_list_does_not_scan_the_whole_table() {
        let db = test_db("conv");
        let p = plan(&db, LIST_CONVERSATIONS_PAGE_SQL);
        assert!(
            !p.contains("SCAN conversations"),
            "会话列表退回全表扫描了：{p}"
        );
        assert!(
            !p.contains("TEMP B-TREE"),
            "会话列表还在临时排序：{p}"
        );
    }

    /// 分页与增量的三条查询同样不许临时排序。
    ///
    /// 往回翻页是 `ORDER BY timestamp DESC, rowid DESC`——靠 SQLite 反向扫同一条
    /// 索引实现。哪天索引没了、或者次序键被改动，这里会红。
    #[test]
    fn the_paging_queries_also_ride_the_index() {
        let db = test_db("paging");
        for (name, sql) in [
            ("尾部一页", MESSAGES_TAIL_SQL),
            ("往回翻页", MESSAGES_BEFORE_SQL),
            ("增量拉取", MESSAGES_SINCE_SQL),
        ] {
            let p = plan(&db, sql);
            assert!(
                p.contains("idx_messages_conversation_timestamp"),
                "{name}没走索引：{p}"
            );
            assert!(!p.contains("TEMP B-TREE"), "{name}还在临时排序：{p}");
        }
    }

    /// 被复合索引完全覆盖的旧单列索引应当已被删除——留着只是每次写入多维护一棵
    /// B 树。这条同时钉住「新库不会再建它」。
    #[test]
    fn the_superseded_single_column_index_is_gone() {
        let db = test_db("stale");
        let conn = db.get_connection().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_messages_conversation_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "旧的单列索引还在，被复合索引覆盖了却没删");
    }
}

/// 增量拉取：只给前端它还没有的那几条。
///
/// 替代的是「每收一个 agent 事件就把整个会话重拉一遍并整体替换」——一轮对话有
/// 几十个这种事件，会话一长就是每事件一次全量读 + 全量重渲染。
#[cfg(test)]
mod messages_delta_tests {
    use super::*;
    use crate::db::DbManager;

    pub(super) fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_delta_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let db = DbManager::new_with_path(path);
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO conversations (id, title, workspace_path, active_agent)
                 VALUES ('c1', 't', 'w', 'a')",
                [],
            )
            .unwrap();
        db
    }

    /// 显式指定 timestamp，好造出「同一秒」的并列。
    pub(super) fn add(db: &DbManager, id: &str, ts: &str) {
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO messages (id, conversation_id, role, content, timestamp)
                 VALUES (?1, 'c1', 'user', ?1, ?2)",
                params![id, ts],
            )
            .unwrap();
    }

    fn ids(d: &MessagesDelta) -> Vec<String> {
        d.messages.iter().map(|m| m.id.clone()).collect()
    }

    #[test]
    fn without_a_cursor_it_returns_everything_and_says_so() {
        let db = test_db("nocursor");
        add(&db, "m1", "2026-01-01 10:00:00");
        add(&db, "m2", "2026-01-01 10:00:01");

        let d = get_messages_since_core(&db, "c1", None).unwrap();
        assert!(d.is_full, "没有游标时必须标明这是全量，否则前端会当成追加");
        assert_eq!(ids(&d), ["m1", "m2"]);
    }

    #[test]
    fn a_cursor_returns_only_what_comes_after_it() {
        let db = test_db("after");
        add(&db, "m1", "2026-01-01 10:00:00");
        add(&db, "m2", "2026-01-01 10:00:01");
        add(&db, "m3", "2026-01-01 10:00:02");

        let d = get_messages_since_core(&db, "c1", Some("m1")).unwrap();
        assert!(!d.is_full, "有有效游标就该是追加");
        assert_eq!(ids(&d), ["m2", "m3"]);
    }

    /// **这条是整个设计的支点。**
    ///
    /// `CURRENT_TIMESTAMP` 是秒级的，一轮对话里连着落库的消息时间戳完全相同
    /// （实测真库 20 条里就有 6 组并列）。只按 timestamp 做游标的话：
    /// `timestamp > ?` 会把同秒的后续消息全部**跳过**，`>=` 则会把游标那条自己
    /// **重复**拉一遍。所以次序键必须是 `(timestamp, rowid)`。
    #[test]
    fn messages_sharing_one_second_are_neither_skipped_nor_repeated() {
        let db = test_db("tie");
        let same = "2026-01-01 10:00:00";
        add(&db, "m1", same);
        add(&db, "m2", same);
        add(&db, "m3", same);
        add(&db, "m4", "2026-01-01 10:00:01");

        let d = get_messages_since_core(&db, "c1", Some("m2")).unwrap();
        assert_eq!(
            ids(&d),
            ["m3", "m4"],
            "同一秒内的次序没接住：m3 被跳过或 m2 被重复"
        );

        // 从并列里的第一条开始，同秒的两条都得来。
        let d = get_messages_since_core(&db, "c1", Some("m1")).unwrap();
        assert_eq!(ids(&d), ["m2", "m3", "m4"]);

        // 从并列里的最后一条开始，只剩下一秒之后的。
        let d = get_messages_since_core(&db, "c1", Some("m3")).unwrap();
        assert_eq!(ids(&d), ["m4"]);
    }

    /// 游标那条被删了（压缩会删旧消息）就退回全量，并**明说**这是全量。
    /// 悄悄退回去的话，前端会把一整段历史当增量追加，界面上直接翻倍。
    #[test]
    fn a_dead_cursor_falls_back_to_full_and_is_not_silent() {
        let db = test_db("dead");
        add(&db, "m1", "2026-01-01 10:00:00");
        add(&db, "m2", "2026-01-01 10:00:01");

        let d = get_messages_since_core(&db, "c1", Some("msg_that_was_compacted_away")).unwrap();
        assert!(d.is_full, "退回全量却没说，前端会当成追加 → 历史翻倍");
        assert_eq!(ids(&d), ["m1", "m2"]);
    }

    /// 别的会话的消息不能漏过来。
    #[test]
    fn the_cursor_is_scoped_to_its_own_conversation() {
        let db = test_db("scope");
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO conversations (id, title, workspace_path, active_agent)
                 VALUES ('c2', 't', 'w', 'a')",
                [],
            )
            .unwrap();
        add(&db, "m1", "2026-01-01 10:00:00");
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO messages (id, conversation_id, role, content, timestamp)
                 VALUES ('other', 'c2', 'user', 'x', '2026-01-01 10:00:05')",
                [],
            )
            .unwrap();

        let d = get_messages_since_core(&db, "c1", Some("m1")).unwrap();
        assert!(d.messages.is_empty(), "串会话了：{:?}", ids(&d));
    }
}

/// 消息分页：最近 N 条，往回翻。
#[cfg(test)]
mod messages_page_tests {
    use super::messages_delta_tests::{add, test_db};
    use super::*;

    fn ids(p: &MessagePage) -> Vec<String> {
        p.messages.iter().map(|m| m.id.clone()).collect()
    }

    fn seed(db: &DbManager, n: usize) {
        for i in 1..=n {
            add(db, &format!("m{i:02}"), &format!("2026-01-01 10:{:02}:00", i));
        }
    }

    /// 初次加载取的是**最新**的一页，不是最早的——聊天是从最新往回看的。
    #[test]
    fn the_first_page_is_the_newest_messages_not_the_oldest() {
        let db = test_db("tail");
        seed(&db, 10);
        let p = get_messages_page_core(&db, "c1", None, 3).unwrap();
        assert_eq!(ids(&p), ["m08", "m09", "m10"], "取成最早的三条了");
        assert_eq!(p.older_remaining, 7, "上面还有 7 条没说");
    }

    #[test]
    fn paging_back_walks_towards_the_oldest() {
        let db = test_db("back");
        seed(&db, 10);
        let p = get_messages_page_core(&db, "c1", Some("m08"), 3).unwrap();
        assert_eq!(ids(&p), ["m05", "m06", "m07"]);
        assert_eq!(p.older_remaining, 4);
    }

    /// 翻到头时 `older_remaining` 必须是 0——界面靠它决定还显不显示「加载更早」。
    #[test]
    fn reaching_the_top_reports_zero_remaining() {
        let db = test_db("top");
        seed(&db, 5);
        let p = get_messages_page_core(&db, "c1", Some("m03"), 10).unwrap();
        assert_eq!(ids(&p), ["m01", "m02"]);
        assert_eq!(p.older_remaining, 0, "已经到顶却还说上面有货");
    }

    /// 同秒并列在分页这边一样会出错：游标不带 rowid 的话，往回翻会跳过或重复。
    #[test]
    fn ties_within_one_second_page_correctly() {
        let db = test_db("pagetie");
        let same = "2026-01-01 10:00:00";
        for id in ["a1", "a2", "a3", "a4"] {
            add(&db, id, same);
        }
        let p = get_messages_page_core(&db, "c1", None, 2).unwrap();
        assert_eq!(ids(&p), ["a3", "a4"]);
        assert_eq!(p.older_remaining, 2);

        let p = get_messages_page_core(&db, "c1", Some("a3"), 2).unwrap();
        assert_eq!(ids(&p), ["a1", "a2"], "同秒内往回翻错位了");
        assert_eq!(p.older_remaining, 0);
    }

    /// limit 来自前端，笔误不能把整张表拉出来——那正是这次要消掉的行为。
    #[test]
    fn the_limit_is_clamped_on_both_ends() {
        let db = test_db("clamp");
        seed(&db, 3);
        assert_eq!(
            get_messages_page_core(&db, "c1", None, 0).unwrap().messages.len(),
            1,
            "0 会让「加载更早」永远点不动"
        );
        assert!(
            get_messages_page_core(&db, "c1", None, 999_999).unwrap().messages.len() <= 500
        );
    }

    /// 游标那条已被压缩删掉时退回最新一页——用户看到的仍是接得上的内容，
    /// 而不是一段悬空的历史。
    #[test]
    fn a_dead_cursor_falls_back_to_the_newest_page() {
        let db = test_db("deadpage");
        seed(&db, 5);
        let p = get_messages_page_core(&db, "c1", Some("compacted_away"), 2).unwrap();
        assert_eq!(ids(&p), ["m04", "m05"]);
    }
}

/// 会话列表：最近 N + 后端搜索（**不做无限滚动**，见 docs/分页方案-W18.md 阶段 3）。
#[cfg(test)]
mod conversation_page_tests {
    use super::*;
    use crate::db::DbManager;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_convpage_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    fn add_conv(db: &DbManager, id: &str, title: &str, archived: bool) {
        db.get_connection()
            .unwrap()
            .execute(
                "INSERT INTO conversations (id, title, workspace_path, active_agent, is_archived)
                 VALUES (?1, ?2, 'w', 'a', ?3)",
                params![id, title, i32::from(archived)],
            )
            .unwrap();
    }

    #[test]
    fn the_list_is_capped_but_the_total_is_still_reported() {
        let db = test_db("cap");
        for i in 0..10 {
            add_conv(&db, &format!("c{i}"), &format!("会话 {i}"), false);
        }
        let page = list_conversations_page_core(&db, false, 3).unwrap();
        assert_eq!(page.conversations.len(), 3);
        assert_eq!(
            page.total, 10,
            "只截断不报总数，用户会以为会话丢了——和消息那边静默截断是同一个毛病"
        );
    }

    #[test]
    fn archived_and_active_do_not_mix() {
        let db = test_db("arch");
        add_conv(&db, "a", "活的", false);
        add_conv(&db, "b", "归档的", true);
        assert_eq!(list_conversations_page_core(&db, false, 10).unwrap().total, 1);
        assert_eq!(list_conversations_page_core(&db, true, 10).unwrap().total, 1);
    }

    #[test]
    fn search_matches_on_title() {
        let db = test_db("search");
        add_conv(&db, "c1", "修复登录接口", false);
        add_conv(&db, "c2", "写周报", false);
        let hits = search_conversations_core(&db, "登录", false, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "c1");
    }

    /// 用户输入里的 `%` / `_` 是**字面量**，不是通配符。不转义的话搜一个下划线
    /// 会把所有会话都搜出来。
    #[test]
    fn wildcards_typed_by_the_user_are_literal() {
        let db = test_db("wild");
        add_conv(&db, "c1", "a_b", false);
        add_conv(&db, "c2", "axb", false);
        add_conv(&db, "c3", "100%完成", false);

        let hits = search_conversations_core(&db, "_", false, 10).unwrap();
        assert_eq!(hits.len(), 1, "下划线被当成通配符了：{:?}",
            hits.iter().map(|c| &c.title).collect::<Vec<_>>());
        assert_eq!(hits[0].id, "c1");

        let hits = search_conversations_core(&db, "%", false, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "c3");
    }

    #[test]
    fn an_empty_query_returns_nothing_rather_than_everything() {
        let db = test_db("empty");
        add_conv(&db, "c1", "x", false);
        assert!(search_conversations_core(&db, "   ", false, 10).unwrap().is_empty());
    }
}
