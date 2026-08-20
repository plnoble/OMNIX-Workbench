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

/// 列表体。归档与未归档只差一个 `is_archived` 取值，以前是两份逐字重复的函数。
///
/// 抽成 `_core` 还有第二个理由：`State<…>` 在单测里构造不出来，命令本身测不到。
/// 会话列表的查询。
///
/// **提成常量是为了让查询计划测试测到真货。** 测试里抄一份 SQL 的话，代码改了
/// 测试照样绿——那种「守卫」守的是自己的副本。
///
/// 不能写 `COALESCE(is_archived, 0) = ?1`：条件列被函数包住就不可 sarg，索引用不上，
/// 计划退回全表 `SCAN`。该列本身是 `INTEGER NOT NULL DEFAULT 0`（建表与 ALTER 两处
/// 都是），不可能为 NULL——那个 COALESCE 是多余的防御，代价却是整张表扫一遍。
pub(crate) const LIST_CONVERSATIONS_SQL: &str = "SELECT id, title, workspace_path, active_agent, created_at
     FROM conversations
     WHERE is_archived = ?1
     ORDER BY created_at DESC";

/// 单会话的消息。靠 `idx_messages_conversation_timestamp` 复合索引直接出有序结果，
/// 不再 `USE TEMP B-TREE FOR ORDER BY`（那是把整个会话排一遍）。
pub(crate) const CONVERSATION_MESSAGES_SQL: &str = "SELECT id, conversation_id, role, content, timestamp, metadata_json
     FROM messages WHERE conversation_id = ?1 ORDER BY timestamp ASC";

pub(crate) fn list_conversations_core(
    db: &DbManager,
    archived: bool,
) -> Result<Vec<ConversationInfo>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(LIST_CONVERSATIONS_SQL)
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![if archived { 1 } else { 0 }], |row| {
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
    for conv in rows.flatten() {
        result.push(conv);
    }
    Ok(result)
}

#[tauri::command]
pub fn get_all_conversations(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ConversationInfo>, String> {
    // Exclude archived conversations from the main list — they show in a separate view
    list_conversations_core(&db, false)
}

#[tauri::command]
pub fn get_archived_conversations(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ConversationInfo>, String> {
    list_conversations_core(&db, true)
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

        let main = list_conversations_core(&db, false).unwrap();
        let archived = list_conversations_core(&db, true).unwrap();
        assert_eq!(ids(&main), vec!["c_kept"], "归档的还留在主列表里");
        assert_eq!(ids(&archived), vec!["c_archived"]);

        set_conversation_archived_core(&db, "c_archived", false).unwrap();
        let main = list_conversations_core(&db, false).unwrap();
        assert_eq!(main.len(), 2, "取消归档后要回到主列表");
        assert!(list_conversations_core(&db, true).unwrap().is_empty());
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
        assert_eq!(ids(&list_conversations_core(&db, false).unwrap()), vec!["c2"]);
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

    fn plan(db: &DbManager, sql: &str) -> String {
        let conn = db.get_connection().unwrap();
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        // 绑个占位值：查询计划不依赖参数的**取值**，但 `?1` 必须有东西绑上，
        // 否则 prepare 出来的语句执行时报 InvalidParameterCount。
        let rows: Vec<String> = stmt
            .query_map(params!["0"], |r| r.get::<_, String>(3))
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
        let p = plan(&db, LIST_CONVERSATIONS_SQL);
        assert!(
            !p.contains("SCAN conversations"),
            "会话列表退回全表扫描了：{p}"
        );
        assert!(
            !p.contains("TEMP B-TREE"),
            "会话列表还在临时排序：{p}"
        );
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
