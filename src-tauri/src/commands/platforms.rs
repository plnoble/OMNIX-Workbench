use super::*;
use crate::db::DbManager;
use crate::input_validation;
use rusqlite::params;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[tauri::command]
pub async fn get_all_models_metadata(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<ModelMetadata>, String> {
    let mut list = Vec::new();

    // 1. Static API Catalog (name, vis, aud, reas, cod, long_ctx, tool, embed, speedy)
    let api_models = vec![
        // OpenAI
        ("gpt-4o", true, true, false, true, true, true, false, false),
        (
            "gpt-4o-mini",
            true,
            true,
            false,
            true,
            true,
            true,
            false,
            true,
        ),
        ("o1", true, false, true, true, true, true, false, false),
        (
            "o1-mini", false, false, true, true, true, true, false, false,
        ),
        ("o3-mini", false, false, true, true, true, true, false, true),
        // Anthropic
        (
            "claude-3-5-sonnet",
            true,
            false,
            false,
            true,
            true,
            true,
            false,
            false,
        ),
        (
            "claude-3-opus",
            true,
            false,
            false,
            true,
            true,
            true,
            false,
            false,
        ),
        (
            "claude-3-5-haiku",
            false,
            false,
            false,
            true,
            true,
            true,
            false,
            true,
        ),
        // DeepSeek
        (
            "deepseek-chat",
            false,
            false,
            false,
            true,
            true,
            true,
            false,
            true,
        ),
        (
            "deepseek-reasoner",
            false,
            false,
            true,
            true,
            true,
            true,
            false,
            false,
        ),
        // Gemini
        (
            "gemini-1.5-pro",
            true,
            true,
            false,
            true,
            true,
            true,
            false,
            false,
        ),
        (
            "gemini-1.5-flash",
            true,
            true,
            false,
            true,
            true,
            true,
            false,
            true,
        ),
        (
            "gemini-2.0-flash",
            true,
            true,
            false,
            true,
            true,
            true,
            false,
            true,
        ),
    ];

    for (name, vis, aud, reas, cod, long_ctx, tool, embed, speedy) in api_models {
        list.push(ModelMetadata {
            name: name.to_string(),
            source: "API".to_string(),
            has_vision: vis,
            has_audio: aud,
            has_reasoning: reas,
            has_coding: cod,
            has_long_context: long_ctx,
            has_tool_use: tool,
            has_embedding: embed,
            has_speedy: speedy,
        });
    }

    // 2. Local Ollama Probe
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
        .map_err(|e| e.to_string())?;

    if let Ok(resp) = client.get("http://localhost:11434/api/tags").send().await {
        if resp.status().is_success() {
            if let Ok(ollama_resp) = resp.json::<OllamaResponse>().await {
                for m in ollama_resp.models {
                    let name_lower = m.name.to_lowercase();

                    // Cap tagging heuristics
                    let has_reasoning = name_lower.contains("r1")
                        || name_lower.contains("reasoning")
                        || name_lower.contains("qwq")
                        || name_lower.contains("thinking");

                    let has_vision = name_lower.contains("vision")
                        || name_lower.contains("llava")
                        || name_lower.contains("minicpm")
                        || name_lower.contains("bakllava")
                        || name_lower.contains("moondream");

                    let has_audio = name_lower.contains("audio") || name_lower.contains("whisper");

                    // Most general-purpose models do coding
                    let has_coding = name_lower.contains("coder")
                        || name_lower.contains("code")
                        || name_lower.contains("llama")
                        || name_lower.contains("qwen")
                        || name_lower.contains("deepseek")
                        || name_lower.contains("mistral")
                        || name_lower.contains("phi")
                        || name_lower.contains("gemma")
                        || name_lower.contains("command-r")
                        || name_lower.contains("starcoder")
                        || name_lower.contains("stable-code");

                    let has_long_context = name_lower.contains("long")
                        || name_lower.contains("128k")
                        || name_lower.contains("32k")
                        || name_lower.contains("64k")
                        || name_lower.contains("yarn")
                        || name_lower.contains("command-r")
                        || name_lower.contains("llama3");

                    let has_tool_use = name_lower.contains("llama3")
                        || name_lower.contains("qwen")
                        || name_lower.contains("mistral")
                        || name_lower.contains("command-r")
                        || name_lower.contains("tool")
                        || name_lower.contains("agent");

                    let has_embedding = name_lower.contains("embed")
                        || name_lower.contains("nomic")
                        || name_lower.contains("bge")
                        || name_lower.contains("mxbai");

                    let has_speedy = name_lower.contains("1.5b")
                        || name_lower.contains("3b")
                        || name_lower.contains("8b")
                        || name_lower.contains("mini")
                        || name_lower.contains("haiku")
                        || name_lower.contains("flash")
                        || name_lower.contains("speed");

                    list.push(ModelMetadata {
                        name: m.name.clone(),
                        source: "Local".to_string(),
                        has_vision,
                        has_audio,
                        has_reasoning,
                        has_coding,
                        has_long_context,
                        has_tool_use,
                        has_embedding,
                        has_speedy,
                    });
                }
            }
        }
    }

    // 3. Load Custom Models from Database
    if let Ok(conn) = db.get_connection() {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT name, source, has_vision, has_audio, has_reasoning,
                    has_coding, has_long_context, has_tool_use, has_embedding, has_speedy
             FROM custom_models",
        ) {
            let rows = stmt.query_map([], |row| {
                let has_vis: i32 = row.get(2)?;
                let has_aud: i32 = row.get(3)?;
                let has_reas: i32 = row.get(4)?;
                let has_cod: i32 = row.get(5)?;
                let has_long: i32 = row.get(6)?;
                let has_tool: i32 = row.get(7)?;
                let has_embed: i32 = row.get(8)?;
                let has_spd: i32 = row.get(9)?;
                Ok(ModelMetadata {
                    name: row.get(0)?,
                    source: row.get(1)?,
                    has_vision: has_vis != 0,
                    has_audio: has_aud != 0,
                    has_reasoning: has_reas != 0,
                    has_coding: has_cod != 0,
                    has_long_context: has_long != 0,
                    has_tool_use: has_tool != 0,
                    has_embedding: has_embed != 0,
                    has_speedy: has_spd != 0,
                })
            });
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    list.push(r);
                }
            }
        }
    }

    Ok(list)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelPlatform {
    pub id: String,
    pub name: String,
    pub api_type: String,
    pub api_key: String,
    pub api_address: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlatformModel {
    pub id: String,
    pub platform_id: String,
    pub model_name: String,
    pub has_vision: bool,
    pub has_audio: bool,
    pub has_reasoning: bool,
    pub has_coding: bool,
    pub has_long_context: bool,
    pub has_tool_use: bool,
    pub has_embedding: bool,
    pub has_speedy: bool,
    pub is_enabled: bool,
    pub status: String,
}

#[tauri::command]
pub fn get_model_platforms(db: State<'_, Arc<DbManager>>) -> Result<Vec<ModelPlatform>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    // 按 priority 降序返回：列表顺序**就是**路由的优先级顺序，用户拖一下就能改。
    // 以前这里不排序，而 priority 在界面上根本无从设置——同名模型挂在多个平台
    // 上时，究竟走哪个既看不见也改不了。
    // **不查 api_key。** 这个命令的返回值会整个过一遍 IPC 到前端，而 Key 没有任何
    // 界面需要它的明文——密钥的增删改查走 `platform_api_keys` 那一套
    // （`list_platform_api_keys` 只回掩码，要看明文得单独调 `reveal_platform_api_key`）。
    // 以前这里连旧列一起 SELECT 出来原样回传，等于每次刷新平台列表都把所有 Key
    // 抄送前端一遍。
    let mut stmt = conn
        .prepare(
            "SELECT id, name, api_type, api_address, is_enabled
             FROM model_platforms ORDER BY priority DESC, name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let is_enabled_int: i32 = row.get(4)?;
            Ok(ModelPlatform {
                id: row.get(0)?,
                name: row.get(1)?,
                api_type: row.get(2)?,
                api_key: String::new(),
                api_address: row.get(3)?,
                is_enabled: is_enabled_int != 0,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(p) = r {
            result.push(p);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn save_model_platform(
    db: State<'_, Arc<DbManager>>,
    platform: ModelPlatform,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    // **不写 api_key 旧列。** `platform.api_key` 仍然是入参（新建平台时表单里顺手
    // 填的那个 Key 走这条命令过来），但它的归宿是 `platform_api_keys`——前端存完
    // 平台紧接着就会调 `add_platform_api_key` 把它加密入库。
    //
    // 以前这里会把同一个 Key **再明文写一遍**到 `model_platforms.api_key`：
    // 读的那一半早就迁到新表了（`platform_keys()` 优先查新表），写的这一半没迁，
    // 于是每加一个平台，密钥就同时以密文和明文两份存在。而 `crypto::decrypt`
    // 对没有 `ENC:` 前缀的值原样返回，明文照样能用——所以一切「工作正常」，
    // 没有任何征兆。
    conn.execute(
        "INSERT INTO model_platforms (id, name, api_type, api_address, is_enabled)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            api_type = excluded.api_type,
            api_address = excluded.api_address,
            is_enabled = excluded.is_enabled",
        params![
            platform.id,
            platform.name,
            platform.api_type,
            platform.api_address,
            if platform.is_enabled { 1 } else { 0 }
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 把存量的 `model_platforms.api_key` 搬进 `platform_api_keys` 并清空旧列。
///
/// 启动时跑一次。返回搬走的 Key 条数（0 = 没有存量，正常）。
///
/// 光把新写入堵住不够——已经躺在库里的明文 Key 不会自己消失，而 `~/.omnix/omnix.db`
/// 会被备份、被云盘同步、被任何能读用户目录的进程打开。
///
/// 判据很干净：`crypto::encrypt` 产出的值必带 `ENC:` 前缀，没有前缀的就是明文。
/// 两种都搬（有前缀的原样搬，没前缀的加密后搬），搬完统一清空旧列。
/// 把 `agent_accounts` 和 `search_providers` 里的明文 Key 就地加密。
///
/// 这两张表的 Key 一直是明文入库的，而平台 Key 在 v0.27 就搬进加密表了——同一个
/// 产品里另一条路没跟上的典型。
///
/// 只加密、不搬表：这两处的 Key 是「一行一把」，没有多 Key 轮换的需求，`crypto`
/// 的密文前缀足够把它们和明文区分开。**读取侧本来就走 `decrypt`（对明文有透传），
/// 所以先迁数据、后迁读法这条路在这里不存在**——这正是上一轮 Auto 路由回归的
/// 成因，不能再来一次。
pub fn migrate_plaintext_secrets_in_place(db: &DbManager) -> Result<usize, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut done = 0usize;
    for (table, id_col) in [("agent_accounts", "id"), ("search_providers", "id")] {
        let rows: Vec<(String, String)> = {
            let sql = format!("SELECT {id_col}, api_key FROM {table}");
            let Ok(mut stmt) = conn.prepare(&sql) else { continue };
            let Ok(mapped) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) else { continue };
            mapped.flatten().collect()
        };
        for (id, stored) in rows {
            if stored.trim().is_empty() {
                continue;
            }
            // 已经是密文的跳过：`decrypt` 解得出**且**结果与原值不同，说明它带前缀。
            let plain = crate::crypto::decrypt(&stored);
            if plain != stored {
                continue;
            }
            let sql = format!("UPDATE {table} SET api_key = ?1 WHERE {id_col} = ?2");
            if conn
                .execute(&sql, params![crate::crypto::encrypt(&plain), id])
                .is_ok()
            {
                done += 1;
            }
        }
    }
    Ok(done)
}

pub fn migrate_legacy_plaintext_keys(db: &DbManager) -> Result<usize, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let rows: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, api_key FROM model_platforms
                 WHERE api_key IS NOT NULL AND TRIM(api_key) <> ''",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };
    if rows.is_empty() {
        return Ok(0);
    }

    let mut moved = 0usize;
    for (platform_id, legacy) in rows {
        // 旧列历来允许逗号分隔多个 Key。
        for (index, raw) in legacy.split(',').map(str::trim).filter(|k| !k.is_empty()).enumerate() {
            let plaintext = crate::crypto::decrypt(raw);
            if plaintext.trim().is_empty() {
                continue;
            }
            // 已经在新表里的就别搬第二遍——迁移可能因为上次崩溃跑了一半。
            //
            // 查重只能问**新表**，不能用 `platform_keys()`：它在新表为空时会回退去
            // 读旧列，而旧列装的正是我们此刻要搬的东西——于是每一条都被判成重复，
            // 一条都搬不动。（这个坑我踩进去过一次，测试才是发现它的原因。）
            let existing: Vec<String> = {
                let mut stmt = conn
                    .prepare("SELECT encrypted_key FROM platform_api_keys WHERE platform_id = ?1")
                    .map_err(|e| e.to_string())?;
                let mapped = stmt
                    .query_map(params![platform_id], |row| row.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                mapped.flatten().map(|v| crate::crypto::decrypt(&v)).collect()
            };
            if existing.iter().any(|k| k == &plaintext) {
                continue;
            }
            let already = existing.len() as i64;
            let id = format!("key_mig_{}_{index}", chrono::Utc::now().timestamp_millis());
            if conn
                .execute(
                    "INSERT INTO platform_api_keys (id, platform_id, encrypted_key, label, is_active)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        id,
                        platform_id,
                        crate::crypto::encrypt(&plaintext),
                        "迁移自旧配置",
                        if already == 0 && index == 0 { 1 } else { 0 }
                    ],
                )
                .is_ok()
            {
                moved += 1;
            }
        }
        // 清空旧列。搬失败的话上面 `moved` 不会涨，但这里照样清——留着明文比丢一个
        // Key 更糟，而 Key 本身用户还能重新填。
        let _ = conn.execute(
            "UPDATE model_platforms SET api_key = '' WHERE id = ?1",
            params![platform_id],
        );
    }
    Ok(moved)
}

#[tauri::command]
pub fn delete_model_platform(db: State<'_, Arc<DbManager>>, id: String) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM model_platforms WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
/// Helper: map a row from platform_models (with all 13 columns) to PlatformModel struct.
/// Column order: id, platform_id, model_name, has_vision, has_audio, has_reasoning,
///   has_coding, has_long_context, has_tool_use, has_embedding, has_speedy, is_enabled, status
fn row_to_platform_model(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlatformModel> {
    let has_vis: i32 = row.get(3)?;
    let has_aud: i32 = row.get(4)?;
    let has_reas: i32 = row.get(5)?;
    let has_cod: i32 = row.get(6)?;
    let has_long: i32 = row.get(7)?;
    let has_tool: i32 = row.get(8)?;
    let has_embed: i32 = row.get(9)?;
    let has_spd: i32 = row.get(10)?;
    let is_enabled_int: i32 = row.get(11)?;
    Ok(PlatformModel {
        id: row.get(0)?,
        platform_id: row.get(1)?,
        model_name: row.get(2)?,
        has_vision: has_vis != 0,
        has_audio: has_aud != 0,
        has_reasoning: has_reas != 0,
        has_coding: has_cod != 0,
        has_long_context: has_long != 0,
        has_tool_use: has_tool != 0,
        has_embedding: has_embed != 0,
        has_speedy: has_spd != 0,
        is_enabled: is_enabled_int != 0,
        status: row.get(12)?,
    })
}

/// Standard SELECT columns for platform_models (13 columns matching row_to_platform_model)
const PM_COLUMNS: &str = "id, platform_id, model_name, has_vision, has_audio, has_reasoning, has_coding, has_long_context, has_tool_use, has_embedding, has_speedy, is_enabled, status";

#[tauri::command]
pub fn get_platform_models(
    db: State<'_, Arc<DbManager>>,
    platform_id: String,
) -> Result<Vec<PlatformModel>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let sql = format!(
        "SELECT {} FROM platform_models WHERE platform_id = ?1",
        PM_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![platform_id], |row| row_to_platform_model(row))
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(m) = r {
            result.push(m);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn save_platform_model(
    db: State<'_, Arc<DbManager>>,
    model: PlatformModel,
) -> Result<(), String> {
    // Auto-infer capabilities from model name (ignore frontend values)
    let caps = infer_capabilities(&model.model_name);

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO platform_models (id, platform_id, model_name, has_vision, has_audio, has_reasoning, has_coding, has_long_context, has_tool_use, has_embedding, has_speedy, is_enabled, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
            has_vision = excluded.has_vision,
            has_audio = excluded.has_audio,
            has_reasoning = excluded.has_reasoning,
            has_coding = excluded.has_coding,
            has_long_context = excluded.has_long_context,
            has_tool_use = excluded.has_tool_use,
            has_embedding = excluded.has_embedding,
            has_speedy = excluded.has_speedy,
            is_enabled = excluded.is_enabled,
            status = excluded.status",
        params![
            model.id,
            model.platform_id,
            model.model_name,
            if caps.has_vision { 1 } else { 0 },
            if caps.has_audio { 1 } else { 0 },
            if caps.has_reasoning { 1 } else { 0 },
            if caps.has_coding { 1 } else { 0 },
            if caps.has_long_context { 1 } else { 0 },
            if caps.has_tool_use { 1 } else { 0 },
            if caps.has_embedding { 1 } else { 0 },
            if caps.has_speedy { 1 } else { 0 },
            if model.is_enabled { 1 } else { 0 },
            model.status
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_platform_model(db: State<'_, Arc<DbManager>>, id: String) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM platform_models WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_active_models(db: State<'_, Arc<DbManager>>) -> Result<Vec<PlatformModel>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let sql = "\
        SELECT \
            pm.id, pm.platform_id, pm.model_name, \
            pm.has_vision, pm.has_audio, pm.has_reasoning, pm.has_coding, \
            pm.has_long_context, pm.has_tool_use, pm.has_embedding, pm.has_speedy, \
            pm.is_enabled, pm.status \
        FROM platform_models pm \
        JOIN model_platforms mp ON pm.platform_id = mp.id \
        WHERE pm.is_enabled = 1 AND mp.is_enabled = 1 \
        ORDER BY mp.name, pm.model_name";
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row_to_platform_model(row))
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(m) = r {
            result.push(m);
        }
    }
    Ok(result)
}

/// 一个可选的模型。`id` 是 `platform_id:model_name` —— **带平台的唯一标识**。
///
/// 以前这里返回 `SELECT DISTINCT model_name`，也就是去重后的裸名字。裸名字不是
/// 唯一标识：同一个模型名可以注册在多个平台上（比如火山方舟同时配了 OpenAI 兼容
/// 和 Anthropic 两个入口）。用户在「内置功能默认模型」里选中的裸名字存进
/// `target_model` 后，网关只能在多个候选平台之间按哈希**静默二选一**，挑中不支持
/// 该模型的那个就是 `Model does not exist`——而用户在界面上根本没有办法表达他
/// 指的是哪一个。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AvailableModel {
    /// `platform_id:model_name`，网关路由本来就认这个格式。
    pub id: String,
    pub model_name: String,
    pub platform_id: String,
    pub platform_name: String,
    /// 同名模型在多个已启用平台上出现时为 true —— 界面据此提醒必须选清楚。
    pub ambiguous: bool,
}

/// Every model the user can point a built-in feature at, qualified by platform.
#[tauri::command]
pub fn get_available_models(db: State<'_, Arc<DbManager>>) -> Result<Vec<AvailableModel>, String> {
    available_models_core(&db)
}

fn available_models_core(db: &DbManager) -> Result<Vec<AvailableModel>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT pm.model_name, pm.platform_id, mp.name \
         FROM platform_models pm \
         JOIN model_platforms mp ON pm.platform_id = mp.id \
         WHERE pm.is_enabled = 1 AND mp.is_enabled = 1 \
         ORDER BY pm.model_name, mp.name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut result: Vec<AvailableModel> = rows
        .flatten()
        .map(|(model_name, platform_id, platform_name)| AvailableModel {
            id: format!("{platform_id}:{model_name}"),
            model_name,
            platform_id,
            platform_name,
            ambiguous: false,
        })
        .collect();

    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for model in &result {
        *seen.entry(model.model_name.as_str()).or_default() += 1;
    }
    let duplicated: std::collections::HashSet<String> = seen
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name.to_string())
        .collect();
    for model in &mut result {
        model.ambiguous = duplicated.contains(&model.model_name);
    }
    Ok(result)
}

fn join_url(base: &str, path: &str) -> String {
    let base_trimmed = base.trim_end_matches('/');
    let path_trimmed = path.trim_start_matches('/');
    format!("{}/{}", base_trimmed, path_trimmed)
}

fn is_volcano_address(api_address: &str) -> bool {
    let lower = api_address.to_ascii_lowercase();
    lower.contains("volces.com")
        || lower.contains("volcengine")
        || lower.contains("ark.cn-")
        || lower.contains("ark.")
}

fn volcano_default_model_names() -> &'static [&'static str] {
    &[
        "doubao-seed-1.6",
        "doubao-seed-1.6-thinking",
        "doubao-1.5-pro-32k",
        "doubao-1.5-pro-256k",
        "doubao-1.5-lite-32k",
        "doubao-pro-32k",
        "doubao-pro-128k",
        "doubao-lite-32k",
        "doubao-lite-128k",
        "doubao-embedding",
    ]
}

/// Capability inference result — 8 bool flags + source indicator
struct CapResult {
    has_vision: bool,
    has_audio: bool,
    has_reasoning: bool,
    has_coding: bool,
    has_long_context: bool,
    has_tool_use: bool,
    has_embedding: bool,
    has_speedy: bool,
}

/// ── Tier 1: Hardcoded model capability catalog ──────────────
/// Covers ~100 mainstream models. Each entry is (id_substring, 8 capabilities).
/// Simplified to our 8-dimension capability model.
const MODEL_CATALOG: &[(&str, (bool, bool, bool, bool, bool, bool, bool, bool))] = &[
    // ── xAI Grok ────────────────────────────────
    ("grok-4.5", (true, false, true, true, true, true, false, false)),
    ("grok-4-fast", (true, false, true, true, true, true, false, true)),
    ("grok-4", (true, false, true, true, true, true, false, false)),
    ("grok-3-mini", (false, false, true, true, false, true, false, true)),
    ("grok-3", (false, false, true, true, true, true, false, false)),
    ("grok-code-fast", (false, false, true, true, true, true, false, true)),
    // ── OpenAI ──────────────────────────────────
    (
        "gpt-4o",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "gpt-4o-mini",
        (true, false, false, true, false, true, false, true),
    ),
    (
        "gpt-4.1",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "gpt-4.1-mini",
        (true, false, false, true, false, true, false, true),
    ),
    (
        "gpt-4.1-nano",
        (true, false, false, true, false, true, false, true),
    ),
    (
        "gpt-4-turbo",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "gpt-4-",
        (false, false, false, true, true, true, false, false),
    ),
    ("o3", (true, false, true, true, true, true, false, false)),
    (
        "o3-mini",
        (false, false, true, true, false, true, false, true),
    ),
    (
        "o4-mini",
        (true, false, true, true, false, true, false, true),
    ),
    ("o1", (true, false, true, true, true, false, false, false)),
    (
        "o1-mini",
        (false, false, true, true, false, false, false, true),
    ),
    (
        "o1-pro",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "gpt-3.5",
        (false, false, false, true, false, true, false, true),
    ),
    // ── Anthropic ───────────────────────────────
    (
        "claude-sonnet-4",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "claude-opus-4",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "claude-3-5-sonnet",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "claude-3-5-haiku",
        (true, false, false, true, false, true, false, true),
    ),
    (
        "claude-3-opus",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "claude-3-sonnet",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "claude-3-haiku",
        (true, false, false, true, false, true, false, true),
    ),
    (
        "claude-3.5",
        (true, false, true, true, true, true, false, false),
    ), // Volcano/Ark alias
    (
        "claude-",
        (true, false, false, true, true, true, false, false),
    ), // generic Claude fallback
    // ── Google Gemini ───────────────────────────
    (
        "gemini-2.5-pro",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "gemini-2.5-flash",
        (true, false, true, true, true, true, false, true),
    ),
    (
        "gemini-2.0-flash",
        (true, false, true, true, true, true, false, true),
    ),
    (
        "gemini-1.5-pro",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "gemini-1.5-flash",
        (true, false, false, true, true, true, false, true),
    ),
    (
        "gemini-1.0-pro",
        (false, false, false, true, false, true, false, false),
    ),
    // ── DeepSeek ────────────────────────────────
    (
        "deepseek-r1",
        (false, false, true, true, true, false, false, false),
    ),
    (
        "deepseek-v3",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "deepseek-chat",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "deepseek-coder",
        (false, false, false, true, false, true, false, false),
    ),
    (
        "deepseek-reasoner",
        (false, false, true, true, true, false, false, false),
    ),
    // ── Qwen ────────────────────────────────────
    (
        "qwen3-",
        (false, false, true, true, true, true, false, false),
    ),
    ("qwq", (false, false, true, true, true, true, false, false)),
    (
        "qwen2.5-",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "qwen2-vl",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "qwen-vl",
        (true, false, false, true, false, true, false, false),
    ),
    (
        "qwen2.5-coder",
        (false, false, false, true, false, true, false, false),
    ),
    // ── Meta Llama ──────────────────────────────
    (
        "llama-4",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "llama-3.3",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "llama-3.2",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "llama-3.1",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "llama-3",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "llama-guard",
        (false, false, false, false, false, false, false, false),
    ),
    // ── Mistral ─────────────────────────────────
    (
        "mistral-large",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "mistral-medium",
        (false, false, false, true, false, true, false, false),
    ),
    (
        "mistral-small",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "mistral-nemo",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "codestral",
        (false, false, false, true, false, true, false, false),
    ),
    (
        "pixtral",
        (true, false, false, true, false, true, false, false),
    ),
    // ── Embedding models ────────────────────────
    (
        "text-embedding",
        (false, false, false, false, false, false, true, true),
    ),
    (
        "embed",
        (false, false, false, false, false, false, true, true),
    ),
    (
        "bge-",
        (false, false, false, false, false, false, true, true),
    ),
    (
        "e5-",
        (false, false, false, false, false, false, true, true),
    ),
    (
        "nomic-embed",
        (false, false, false, false, false, false, true, true),
    ),
    (
        "voyage-",
        (false, false, false, false, false, false, true, true),
    ),
    (
        "mxbai-",
        (false, false, false, false, false, false, true, true),
    ),
    // ── Image generation ────────────────────────
    (
        "dall-e",
        (false, false, false, false, false, false, false, false),
    ),
    (
        "flux",
        (false, false, false, false, false, false, false, false),
    ),
    (
        "stable-diffusion",
        (false, false, false, false, false, false, false, false),
    ),
    // ── Audio ───────────────────────────────────
    (
        "whisper",
        (false, true, false, false, false, false, false, false),
    ),
    (
        "tts-",
        (false, false, false, false, false, false, false, false),
    ),
    // ── Zhipu GLM ───────────────────────────────
    (
        "glm-5.",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "glm-4v",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "glm-4-",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "glm-4-flash",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "glm-z1",
        (false, false, true, true, false, true, false, false),
    ),
    (
        "glm-",
        (false, false, false, true, true, true, false, false),
    ),
    // ── Moonshot / Kimi ─────────────────────────
    (
        "kimi-k2",
        (true, false, true, true, true, true, false, false),
    ),
    (
        "moonshot-v1",
        (false, false, false, true, true, true, false, false),
    ),
    // ── Doubao / Volcengine ─────────────────────
    (
        "doubao-seed",
        (false, false, true, true, true, true, false, false),
    ),
    (
        "doubao-pro",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "doubao-lite",
        (false, false, false, true, false, true, false, true),
    ),
    // ── MiniMax ─────────────────────────────────
    (
        "minimax-text",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "minimax-m1",
        (false, false, true, true, true, true, false, false),
    ),
    // ── Grok ────────────────────────────────────
    (
        "grok-3-mini",
        (false, false, true, true, false, true, false, true),
    ),
    (
        "grok-3",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "grok-4",
        (true, false, true, true, true, true, false, false),
    ),
    // ── Yi / 01.ai ─────────────────────────────
    (
        "yi-vision",
        (true, false, false, true, true, true, false, false),
    ),
    (
        "yi-lightning",
        (false, false, false, true, false, true, false, true),
    ),
    // ── Hunyuan ─────────────────────────────────
    (
        "hunyuan-t1",
        (false, false, true, true, true, true, false, false),
    ),
    (
        "hunyuan-pro",
        (false, false, false, true, true, true, false, false),
    ),
    (
        "hunyuan-lite",
        (false, false, false, true, false, true, false, true),
    ),
    // ── Step ────────────────────────────────────
    (
        "step-2",
        (false, false, false, true, true, true, false, false),
    ),
    // ── Baichuan ────────────────────────────────
    (
        "baichuan4",
        (false, false, false, true, true, true, false, false),
    ),
    // ── Perplexity ──────────────────────────────
    (
        "sonar",
        (false, false, false, true, false, true, false, true),
    ),
    // ── Mimo ────────────────────────────────────
    (
        "mimo-vl",
        (true, false, true, true, true, true, false, false),
    ),
    // ── Local models ────────────────────────────
    (
        "llava",
        (true, false, false, true, false, false, false, false),
    ),
    (
        "bakllava",
        (true, false, false, true, false, false, false, false),
    ),
    (
        "moondream",
        (true, false, false, true, false, false, false, false),
    ),
    (
        "minicpm-v",
        (true, false, false, true, false, false, false, false),
    ),
    (
        "starcoder",
        (false, false, false, true, false, false, false, false),
    ),
    (
        "codellama",
        (false, false, false, true, false, true, false, false),
    ),
    (
        "phi-3",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "gemma-2",
        (false, false, false, true, false, true, false, true),
    ),
    (
        "command-r",
        (false, false, false, true, true, true, false, false),
    ),
    // ── Reranking ───────────────────────────────
    (
        "rerank",
        (false, false, false, false, false, false, true, false),
    ),
];

/// Infer 8-dimension capability flags from a model name.
/// Phase 1: Match against hardcoded catalog (most accurate).
/// Phase 2: Fall back to enhanced name heuristics.
fn infer_capabilities(name: &str) -> CapResult {
    let n = name.to_lowercase();

    // Phase 1: Catalog lookup — match the longest substring first (most specific)
    let mut best_match: Option<(&str, (bool, bool, bool, bool, bool, bool, bool, bool))> = None;
    let mut best_len = 0usize;
    for (pattern, caps) in MODEL_CATALOG {
        if n.contains(*pattern) && pattern.len() > best_len {
            best_match = Some((*pattern, *caps));
            best_len = pattern.len();
        }
    }
    if let Some((pattern, (v, a, r, c, lc, t, e, s))) = best_match {
        log::warn!("[Capabilities] Catalog match: '{}' → '{}'", name, pattern);
        return CapResult {
            has_vision: v,
            has_audio: a,
            has_reasoning: r,
            has_coding: c,
            has_long_context: lc,
            has_tool_use: t,
            has_embedding: e,
            has_speedy: s,
        };
    }

    // Phase 2: Heuristic fallback (regex heuristics)
    let has_vision = n.contains("vision")
        || n.contains("vl")
        || n.contains("4o")
        || n.contains("llava")
        || n.contains("minicpm")
        || n.contains("bakllava")
        || n.contains("moondream")
        || n.contains("pixtral")
        || n.contains("-v")
        || n.contains("image")
        || n.contains("multimodal");
    let has_audio = n.contains("audio") || n.contains("whisper") || n.contains("tts");
    let has_reasoning = n.contains("r1")
        || n.contains("reason")
        || n.contains("o1")
        || n.contains("o3")
        || n.contains("o4")
        || n.contains("qwq")
        || n.contains("thinking")
        || n.contains("deepthink")
        || n.contains("-z1")
        || n.contains("t1")
        || n.contains("seed")
        || n.contains("reasoner");
    let has_coding =
        n.contains("coder") || n.contains("code") || n.contains("dev") || n.contains("programming");
    let has_long_context = n.contains("128k")
        || n.contains("32k")
        || n.contains("64k")
        || n.contains("200k")
        || n.contains("long")
        || n.contains("yarn");
    let has_tool_use = n.contains("tool") || n.contains("agent") || n.contains("function");
    let has_embedding = n.contains("embed")
        || n.contains("nomic")
        || n.contains("bge")
        || n.contains("mxbai")
        || n.contains("e5")
        || n.contains("rerank");
    let has_speedy = n.contains("mini")
        || n.contains("flash")
        || n.contains("haiku")
        || n.contains("nano")
        || n.contains("lite")
        || n.contains("speed")
        || n.contains("1.5b")
        || n.contains("3b")
        || n.contains("7b")
        || n.contains("8b");

    CapResult {
        has_vision,
        has_audio,
        has_reasoning,
        has_coding,
        has_long_context,
        has_tool_use,
        has_embedding,
        has_speedy,
    }
}

#[tauri::command]
pub async fn fetch_remote_models(
    db: State<'_, Arc<DbManager>>,
    platform_id: String,
) -> Result<Vec<PlatformModel>, String> {
    let (api_type, api_key, api_address) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT api_type, api_key, api_address FROM model_platforms WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![platform_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
    };
    let api_key = crate::crypto::decrypt(&api_key);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let mut model_names = Vec::new();

    // ── Strategy: fetch models based on api_type ────────────
    if is_volcano_address(&api_address) {
        for name in volcano_default_model_names() {
            model_names.push((*name).to_string());
        }
    } else {
        match api_type.as_str() {
            "ollama" => {
                let url = join_url(&api_address, "/api/tags");
                if let Ok(resp) = client.get(&url).send().await {
                    if resp.status().is_success() {
                        #[derive(serde::Deserialize)]
                        struct OllamaModel {
                            name: String,
                        }
                        #[derive(serde::Deserialize)]
                        struct OllamaTags {
                            models: Vec<OllamaModel>,
                        }
                        if let Ok(tags) = resp.json::<OllamaTags>().await {
                            for m in tags.models {
                                model_names.push(m.name);
                            }
                        }
                    }
                }
            }
            "openai" | "openai-response" | "openai-compatible" => {
                // ── Volcano (火山方舟) special handling ─────────────────────────
                // Volcano's "codingplan" / "/api/coding" endpoints are CHAT endpoints,
                // not model-list endpoints. Calling /api/v3/models returns the WHOLE
                // tenant's models (including proxied Claude/GPT/etc.), which is
                // WRONG for codingplan users — they want their doubao endpoint IDs
                // and custom inference profiles, not random tenant models.
                //
                // Per user feedback (Round 5 + 6): we MUST NOT auto-fetch from the
                // tenant URL. Instead, return the well-known doubao model family
                // and let the user add their own endpoint IDs in the Model Modal.
                let is_volcano =
                    api_address.contains("volces.com") || api_address.contains("ark.cn-beijing");
                if is_volcano {
                    for name in [
                        "doubao-seed-1.6",
                        "doubao-1.5-pro-32k",
                        "doubao-1.5-pro-256k",
                        "doubao-1.5-lite-32k",
                        "doubao-pro-32k",
                        "doubao-pro-128k",
                        "doubao-lite-32k",
                        "doubao-lite-128k",
                        "doubao-embedding",
                    ] {
                        model_names.push(name.to_string());
                    }
                    // Skip the generic OpenAI path entirely — Volcano endpoint IDs
                    // are tenant-specific and cannot be discovered safely.
                } else {
                    // Generic OpenAI-compatible: try /models on the user's address.
                    let primary_url = join_url(&api_address, "/models");
                    let mut req = client.get(&primary_url);
                    if !api_key.trim().is_empty() {
                        req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
                    }
                    match req.send().await {
                        Ok(resp) if resp.status().is_success() => {
                            #[derive(serde::Deserialize)]
                            struct OpenAIModel {
                                id: String,
                            }
                            #[derive(serde::Deserialize)]
                            struct OpenAIModels {
                                data: Vec<OpenAIModel>,
                            }
                            if let Ok(models_list) = resp.json::<OpenAIModels>().await {
                                for m in models_list.data {
                                    model_names.push(m.id);
                                }
                            }
                        }
                        Ok(resp) => {
                            return Err(format!(
                                "模型列表 API 返回错误 (HTTP {}), 请检查 API Key 和地址",
                                resp.status().as_u16()
                            ));
                        }
                        Err(e) => {
                            return Err(format!(
                                "模型列表 API 请求失败: {}, 请检查网络和 API 地址",
                                e
                            ));
                        }
                    }
                }
            }
            "anthropic" => {
                let url = join_url(&api_address, "/v1/models");
                let mut fetched = false;
                let mut req = client.get(&url);
                if !api_key.trim().is_empty() {
                    req = req
                        .header("x-api-key", api_key.trim())
                        .header("anthropic-version", "2023-06-01");
                }
                if let Ok(resp) = req.send().await {
                    if resp.status().is_success() {
                        #[derive(serde::Deserialize)]
                        struct AntModel {
                            id: String,
                        }
                        #[derive(serde::Deserialize)]
                        struct AntModels {
                            data: Vec<AntModel>,
                        }
                        if let Ok(models_list) = resp.json::<AntModels>().await {
                            for m in models_list.data {
                                model_names.push(m.id);
                            }
                            fetched = true;
                        }
                    }
                }
                if !fetched {
                    // Fallback: known Anthropic models
                    for name in [
                        "claude-sonnet-4-20250514",
                        "claude-3-5-sonnet-20241022",
                        "claude-3-5-haiku-20241022",
                        "claude-3-opus-20240229",
                        "claude-3-5-sonnet",
                        "claude-3-5-haiku",
                    ] {
                        model_names.push(name.to_string());
                    }
                }
            }
            "gemini" => {
                // Gemini: GET {base}/v1beta/models?key={api_key}
                let base = api_address.trim_end_matches('/');
                let url = format!("{}/v1beta/models?key={}", base, api_key.trim());
                if let Ok(resp) = client.get(&url).send().await {
                    if resp.status().is_success() {
                        #[derive(serde::Deserialize)]
                        struct GeminiModel {
                            name: String,
                        }
                        #[derive(serde::Deserialize)]
                        struct GeminiModels {
                            models: Vec<GeminiModel>,
                        }
                        if let Ok(models_list) = resp.json::<GeminiModels>().await {
                            for m in models_list.models {
                                // Strip "models/" prefix from Gemini API response
                                let name = m
                                    .name
                                    .strip_prefix("models/")
                                    .unwrap_or(&m.name)
                                    .to_string();
                                model_names.push(name);
                            }
                        }
                    }
                }
            }
            "mistral" => {
                let url = join_url(&api_address, "/v1/models");
                let mut req = client.get(&url);
                if !api_key.trim().is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
                }
                if let Ok(resp) = req.send().await {
                    if resp.status().is_success() {
                        #[derive(serde::Deserialize)]
                        struct MistralModel {
                            id: String,
                        }
                        #[derive(serde::Deserialize)]
                        struct MistralModels {
                            data: Vec<MistralModel>,
                        }
                        if let Ok(models_list) = resp.json::<MistralModels>().await {
                            for m in models_list.data {
                                model_names.push(m.id);
                            }
                        }
                    }
                }
            }
            "new-api" => {
                // new-api gateways use the same /v1/models as OpenAI
                let url = join_url(&api_address, "/v1/models");
                let mut req = client.get(&url);
                if !api_key.trim().is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
                }
                if let Ok(resp) = req.send().await {
                    if resp.status().is_success() {
                        #[derive(serde::Deserialize)]
                        struct NewApiModel {
                            id: String,
                        }
                        #[derive(serde::Deserialize)]
                        struct NewApiModels {
                            data: Vec<NewApiModel>,
                        }
                        if let Ok(models_list) = resp.json::<NewApiModels>().await {
                            for m in models_list.data {
                                model_names.push(m.id);
                            }
                        }
                    }
                }
            }
            "azure-openai" => {
                // Azure OpenAI does not support a model list API
                // Return empty — user must add models manually
            }
            _ => {
                // Default fallback: try OpenAI-compatible /models
                let url = join_url(&api_address, "/models");
                let mut req = client.get(&url);
                if !api_key.trim().is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
                }
                if let Ok(resp) = req.send().await {
                    if resp.status().is_success() {
                        #[derive(serde::Deserialize)]
                        struct FallbackModel {
                            id: String,
                        }
                        #[derive(serde::Deserialize)]
                        struct FallbackModels {
                            data: Vec<FallbackModel>,
                        }
                        if let Ok(models_list) = resp.json::<FallbackModels>().await {
                            for m in models_list.data {
                                model_names.push(m.id);
                            }
                        }
                    }
                }
            }
        }
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut imported_models = Vec::new();

    if !model_names.is_empty() {
        conn.execute(
            "DELETE FROM platform_models WHERE platform_id = ?1",
            params![&platform_id],
        )
        .map_err(|e| e.to_string())?;
    }

    for name in model_names {
        let id = format!("{}::{}", platform_id, name);
        let caps = infer_capabilities(&name);

        let pm = PlatformModel {
            id: id.clone(),
            platform_id: platform_id.clone(),
            model_name: name.clone(),
            has_vision: caps.has_vision,
            has_audio: caps.has_audio,
            has_reasoning: caps.has_reasoning,
            has_coding: caps.has_coding,
            has_long_context: caps.has_long_context,
            has_tool_use: caps.has_tool_use,
            has_embedding: caps.has_embedding,
            has_speedy: caps.has_speedy,
            is_enabled: true,
            status: "unknown".to_string(),
        };

        let _ = conn.execute(
            "INSERT INTO platform_models (id, platform_id, model_name, has_vision, has_audio, has_reasoning, has_coding, has_long_context, has_tool_use, has_embedding, has_speedy, is_enabled, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                model_name = excluded.model_name,
                has_vision = excluded.has_vision,
                has_audio = excluded.has_audio,
                has_reasoning = excluded.has_reasoning,
                has_coding = excluded.has_coding,
                has_long_context = excluded.has_long_context,
                has_tool_use = excluded.has_tool_use,
                has_embedding = excluded.has_embedding,
                has_speedy = excluded.has_speedy",
            params![
                id,
                platform_id,
                name,
                if caps.has_vision { 1 } else { 0 },
                if caps.has_audio { 1 } else { 0 },
                if caps.has_reasoning { 1 } else { 0 },
                if caps.has_coding { 1 } else { 0 },
                if caps.has_long_context { 1 } else { 0 },
                if caps.has_tool_use { 1 } else { 0 },
                if caps.has_embedding { 1 } else { 0 },
                if caps.has_speedy { 1 } else { 0 },
                1,
                "unknown"
            ],
        );
        imported_models.push(pm);
    }

    Ok(imported_models)
}

/// Detailed health check result — distinguishes between truly usable,
/// auth-failed, rate-limited, unreachable, and no-key states.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthCheckDetail {
    pub status: String, // success | auth_error | rate_limited | error | unreachable | no_api_key
    pub http_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub message: String,
}

/// Classify an HTTP response code into a health status.
/// Only HTTP 200 counts as "success". 401/403 = auth_error, 429 = rate_limited,
/// everything else = error.
fn classify_http_status(code: u16) -> &'static str {
    match code {
        200..=299 => "success",
        401 | 403 => "auth_error",
        429 => "rate_limited",
        _ => "error",
    }
}

/// Build a HealthCheckDetail from a successful HTTP response.
fn health_from_response(
    code: u16,
    _latency: std::time::Instant,
    start: std::time::Instant,
) -> HealthCheckDetail {
    let status = classify_http_status(code);
    let msg = match status {
        "success" => "模型可用".into(),
        "auth_error" => format!("认证失败 (HTTP {})", code),
        "rate_limited" => "限流中，稍后重试".into(),
        _ => format!("请求错误 (HTTP {})", code),
    };
    HealthCheckDetail {
        status: status.into(),
        http_code: Some(code),
        latency_ms: Some(start.elapsed().as_millis() as u64),
        message: msg,
    }
}

#[tauri::command]
pub async fn check_model_status(
    db: State<'_, Arc<DbManager>>,
    model_id: String,
) -> Result<HealthCheckDetail, String> {
    let (platform_id, model_name, api_type, api_address) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT pm.platform_id, pm.model_name, mp.api_type, mp.api_address
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.id = ?1",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![model_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
    };
    // 和网关跑真实请求时用的是同一套解析（活跃 Key 在前）。以前这里只读
    // `model_platforms.api_key` 旧列，测的 Key 和实际跑的可以不是同一个。
    let api_key = platform_keys(&db, &platform_id)
        .0
        .into_iter()
        .next()
        .unwrap_or_default();

    // No API Key → skip request, return no_api_key immediately
    if api_type != "ollama" && api_key.trim().is_empty() {
        let detail = HealthCheckDetail {
            status: "no_api_key".into(),
            http_code: None,
            latency_ms: None,
            message: "未配置 API Key".into(),
        };
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "UPDATE platform_models SET status = ?1 WHERE id = ?2",
            params!["no_api_key", model_id],
        );
        return Ok(detail);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let start = std::time::Instant::now();
    let mut detail = HealthCheckDetail {
        status: "unreachable".into(),
        http_code: None,
        latency_ms: None,
        message: "连接超时或网络不可达".into(),
    };

    match api_type.as_str() {
        "ollama" => {
            let url = join_url(&api_address, "/api/chat");
            let body = serde_json::json!({
                "model": model_name,
                "messages": [{"role": "user", "content": "ping"}],
                "stream": false
            });
            if let Ok(resp) = client.post(&url).json(&body).send().await {
                detail = health_from_response(resp.status().as_u16(), start, start);
            }
        }
        "gemini" => {
            let base = api_address.trim_end_matches('/');
            let url = format!(
                "{}/v1beta/models/{}:generateContent?key={}",
                base,
                model_name,
                api_key.trim()
            );
            let body = serde_json::json!({
                "contents": [{"parts": [{"text": "ping"}]}],
                "generationConfig": {"maxOutputTokens": 1}
            });
            if let Ok(resp) = client.post(&url).json(&body).send().await {
                detail = health_from_response(resp.status().as_u16(), start, start);
            }
        }
        "anthropic" => {
            let url = join_url(&api_address, "/v1/messages");
            let body = serde_json::json!({
                "model": model_name,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1
            });
            let mut req = client
                .post(&url)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json");
            if !api_key.trim().is_empty() {
                req = req.header("x-api-key", api_key.trim());
            }
            if let Ok(resp) = req.json(&body).send().await {
                detail = health_from_response(resp.status().as_u16(), start, start);
            }
        }
        _ => {
            let url = join_url(&api_address, "/chat/completions");
            let body = serde_json::json!({
                "model": model_name,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1
            });
            let mut req = client.post(&url);
            if !api_key.trim().is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
            }
            if let Ok(resp) = req.json(&body).send().await {
                detail = health_from_response(resp.status().as_u16(), start, start);
            }
        }
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let _ = conn.execute(
        "UPDATE platform_models SET status = ?1 WHERE id = ?2",
        params![detail.status, model_id],
    );

    Ok(detail)
}

/// Batch health check: concurrently test all models under a platform and return updated list.
#[tauri::command]
pub async fn batch_check_models(
    db: State<'_, Arc<DbManager>>,
    platform_id: String,
) -> Result<Vec<PlatformModel>, String> {
    let (api_type, api_key, api_address) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT api_type, api_key, api_address FROM model_platforms WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![platform_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
    };
    let api_key = crate::crypto::decrypt(&api_key);

    // No API Key → mark all models as no_api_key without making requests
    if api_type != "ollama" && api_key.trim().is_empty() {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "UPDATE platform_models SET status = 'no_api_key' WHERE platform_id = ?1",
            params![platform_id],
        );
        // Return updated list
        let sql = format!(
            "SELECT {} FROM platform_models WHERE platform_id = ?1",
            PM_COLUMNS
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![platform_id], |row| row_to_platform_model(row))
            .map_err(|e| e.to_string())?;
        return Ok(rows.filter_map(|r| r.ok()).collect());
    }

    // Load all models for this platform
    let models: Vec<(String, String)> = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, model_name FROM platform_models WHERE platform_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![platform_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    // Ping each model concurrently
    let mut handles = Vec::new();
    for (model_id, model_name) in &models {
        let mid = model_id.clone();
        let mname = model_name.clone();
        let atype = api_type.clone();
        let akey = api_key.clone();
        let aaddr = api_address.clone();
        let cl = client.clone();
        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();
            let mut status = "unreachable";

            match atype.as_str() {
                "ollama" => {
                    let url = join_url(&aaddr, "/api/chat");
                    let body = serde_json::json!({"model": mname, "messages": [{"role": "user", "content": "ping"}], "stream": false});
                    if let Ok(resp) = cl.post(&url).json(&body).send().await {
                        status = classify_http_status(resp.status().as_u16());
                    }
                }
                "gemini" => {
                    let base = aaddr.trim_end_matches('/');
                    let url = format!(
                        "{}/v1beta/models/{}:generateContent?key={}",
                        base,
                        mname,
                        akey.trim()
                    );
                    let body = serde_json::json!({"contents": [{"parts": [{"text": "ping"}]}], "generationConfig": {"maxOutputTokens": 1}});
                    if let Ok(resp) = cl.post(&url).json(&body).send().await {
                        status = classify_http_status(resp.status().as_u16());
                    }
                }
                "anthropic" => {
                    let url = join_url(&aaddr, "/v1/messages");
                    let body = serde_json::json!({"model": mname, "messages": [{"role": "user", "content": "ping"}], "max_tokens": 1});
                    let mut req = cl
                        .post(&url)
                        .header("anthropic-version", "2023-06-01")
                        .header("content-type", "application/json");
                    if !akey.trim().is_empty() {
                        req = req.header("x-api-key", akey.trim());
                    }
                    if let Ok(resp) = req.json(&body).send().await {
                        status = classify_http_status(resp.status().as_u16());
                    }
                }
                _ => {
                    let url = join_url(&aaddr, "/chat/completions");
                    let body = serde_json::json!({"model": mname, "messages": [{"role": "user", "content": "ping"}], "max_tokens": 1});
                    let mut req = cl.post(&url);
                    if !akey.trim().is_empty() {
                        req = req.header("Authorization", format!("Bearer {}", akey.trim()));
                    }
                    if let Ok(resp) = req.json(&body).send().await {
                        status = classify_http_status(resp.status().as_u16());
                    }
                }
            }
            let _ = start; // latency tracked implicitly via timeout
            (mid, status.to_string())
        });
        handles.push(handle);
    }

    // Collect results
    let mut results = Vec::new();
    for h in handles {
        if let Ok((model_id, status)) = h.await {
            results.push((model_id, status));
        }
    }

    // Batch update DB
    {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        for (model_id, status) in &results {
            let _ = conn.execute(
                "UPDATE platform_models SET status = ?1 WHERE id = ?2",
                params![status, model_id],
            );
        }
    }

    // Return the updated full model list
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let sql = format!(
        "SELECT {} FROM platform_models WHERE platform_id = ?1",
        PM_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![platform_id], |row| row_to_platform_model(row))
        .map_err(|e| e.to_string())?;
    let mut updated = Vec::new();
    for r in rows {
        if let Ok(m) = r {
            updated.push(m);
        }
    }
    Ok(updated)
}

/// Re-infer capabilities for a specific model or all models under a platform.
/// Returns the number of models updated.
#[tauri::command]
pub fn reinfer_model_capabilities(
    model_id: Option<String>,
    platform_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<u32, String> {
    // Phase 1: Collect models to re-infer
    let models: Vec<(String, String)> = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        if let Some(mid) = model_id {
            let name: String = conn
                .query_row(
                    "SELECT model_name FROM platform_models WHERE id = ?1",
                    params![mid],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            vec![(mid, name)]
        } else if let Some(pid) = platform_id {
            let mut stmt = conn
                .prepare("SELECT id, model_name FROM platform_models WHERE platform_id = ?1")
                .map_err(|e| e.to_string())?;
            let rows: Vec<_> = stmt
                .query_map(params![pid], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        } else {
            let mut stmt = conn
                .prepare("SELECT id, model_name FROM platform_models")
                .map_err(|e| e.to_string())?;
            let rows: Vec<_> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        }
    }; // conn dropped here

    // Phase 2: Update capabilities
    let mut count = 0u32;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    for (id, name) in &models {
        let caps = infer_capabilities(name);
        let result = conn.execute(
            "UPDATE platform_models SET has_vision=?1, has_audio=?2, has_reasoning=?3, has_coding=?4, has_long_context=?5, has_tool_use=?6, has_embedding=?7, has_speedy=?8 WHERE id=?9",
            params![
                if caps.has_vision { 1 } else { 0 },
                if caps.has_audio { 1 } else { 0 },
                if caps.has_reasoning { 1 } else { 0 },
                if caps.has_coding { 1 } else { 0 },
                if caps.has_long_context { 1 } else { 0 },
                if caps.has_tool_use { 1 } else { 0 },
                if caps.has_embedding { 1 } else { 0 },
                if caps.has_speedy { 1 } else { 0 },
                id,
            ],
        );
        if let Ok(_) = result {
            count += 1;
        }
    }

    Ok(count)
}


#[cfg(test)]
mod available_model_tests {
    use crate::db::DbManager;
    use std::sync::Arc;

    fn temp_db(tag: &str) -> (Arc<DbManager>, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "omnix_avail_{tag}_{}_{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        let _ = std::fs::remove_file(&path);
        (Arc::new(DbManager::new_with_path(path.clone())), path)
    }

    fn install(db: &DbManager, platform: &str, label: &str, model: &str) {
        let conn = db.get_connection().expect("db");
        let _ = conn.execute(
            "INSERT OR IGNORE INTO model_platforms (id, name, api_type, api_key, api_address, is_enabled)
             VALUES (?1, ?2, 'openai', 'k', 'http://x', 1)",
            rusqlite::params![platform, label],
        );
        conn.execute(
            "INSERT INTO platform_models (id, platform_id, model_name, is_enabled)
             VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![format!("{platform}:{model}"), platform, model],
        )
        .expect("插入模型");
    }

    /// 同一个模型名注册在两个平台上时，两条都要出现，且都标成 ambiguous。
    ///
    /// 原 bug：这里返回 `SELECT DISTINCT model_name`，两个平台合并成一行裸名字。
    /// 用户选中它存进 `target_model`，网关只能按哈希在两个平台里静默二选一——
    /// 挑中不支持该模型的那个就是 `Model does not exist`，而用户在界面上根本
    /// 没有办法表达他指的是哪一个。
    #[test]
    fn a_model_name_on_two_platforms_stays_distinguishable() {
        let (db, path) = temp_db("dup");
        {
            let conn = db.get_connection().expect("db");
            let _ = conn.execute("DELETE FROM platform_models", []);
            let _ = conn.execute("DELETE FROM model_platforms", []);
        }
        install(&db, "ark_openai", "火山方舟(OpenAI 兼容)", "deepseek-v4-pro");
        install(&db, "ark_anthropic", "火山", "deepseek-v4-pro");
        install(&db, "ark_openai", "火山方舟(OpenAI 兼容)", "只此一家");

        let models = super::available_models_core(&db).expect("列表");

        let dup: Vec<_> = models.iter().filter(|m| m.model_name == "deepseek-v4-pro").collect();
        assert_eq!(dup.len(), 2, "两个平台各自一行，不能去重合并：{models:?}");
        assert!(dup.iter().all(|m| m.ambiguous), "同名的都要标出来：{dup:?}");

        let ids: Vec<&str> = dup.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"ark_openai:deepseek-v4-pro"), "{ids:?}");
        assert!(ids.contains(&"ark_anthropic:deepseek-v4-pro"), "{ids:?}");

        let unique = models.iter().find(|m| m.model_name == "只此一家").expect("独苗");
        assert!(!unique.ambiguous, "只在一个平台上的不该被标成有歧义");

        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

/// 一个平台可用的 API Key，按使用顺序：活跃的在前。
///
/// **这是唯一一处解析 Key 的地方。** 以前有三份各不相同的实现：会话网关读
/// `platform_api_keys` 新表（回落旧列按逗号切分），legacy 路由只读 `model_platforms.api_key`
/// 旧列，健康检测也只读旧列。同一个平台两处存的 Key 可以不一样——于是「⚡测试」
/// 测的 Key 根本不是实际跑的那个，绿灯是假的；页头宣传的「主 Key + 故障切换」
/// 也只在会话网关那一条路上成立。
pub fn platform_keys(db: &DbManager, platform_id: &str) -> (Vec<String>, Vec<Option<String>>) {
    let mut keys = Vec::new();
    let mut key_ids = Vec::new();
    let Ok(conn) = db.get_connection() else {
        return (keys, key_ids);
    };

    if let Ok(mut statement) = conn.prepare(
        "SELECT id, encrypted_key FROM platform_api_keys
         WHERE platform_id = ?1 ORDER BY is_active DESC, created_at ASC",
    ) {
        if let Ok(rows) = statement.query_map(params![platform_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for (id, encrypted) in rows.flatten() {
                let key = crate::crypto::decrypt(&encrypted);
                if !key.trim().is_empty() && !keys.contains(&key) {
                    keys.push(key);
                    key_ids.push(Some(id));
                }
            }
        }
    }

    // 旧列作为回落：老配置还没迁到 platform_api_keys，逗号分隔的多 Key 也在这。
    if keys.is_empty() {
        if let Ok(legacy) = conn.query_row(
            "SELECT api_key FROM model_platforms WHERE id = ?1",
            params![platform_id],
            |row| row.get::<_, String>(0),
        ) {
            for encrypted in legacy.split(',').map(str::trim).filter(|k| !k.is_empty()) {
                let key = crate::crypto::decrypt(encrypted);
                if !key.trim().is_empty() && !keys.contains(&key) {
                    keys.push(key);
                    key_ids.push(None);
                }
            }
        }
    }
    (keys, key_ids)
}

/// 一行模型在**路由**眼里长什么样。模型中心据此把「会走哪个平台、为什么」
/// 摆到明面上——以前这些只活在 `proxy.rs` 里，界面一个字都不说。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelRouting {
    /// `platform_models.id`
    pub model_id: String,
    pub model_name: String,
    /// 其它也提供这个模型名的**已启用**平台显示名（不含自己）。
    pub rival_platforms: Vec<String>,
    /// 只写裸模型名时，路由当前会选中的平台 id。等于本行所属平台即「当前赢家」。
    pub winner_platform_id: Option<String>,
    /// `request_logs` 里这个模型最近一次失败的上游原话。
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
}

#[tauri::command]
pub fn get_model_routing(
    db: State<'_, Arc<DbManager>>,
    platform_id: String,
) -> Result<Vec<ModelRouting>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT id, model_name FROM platform_models WHERE platform_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![platform_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();

    let mut result = Vec::new();
    for (model_id, model_name) in rows {
        let mut rival_stmt = conn
            .prepare(
                "SELECT mp.name FROM platform_models pm
                 JOIN model_platforms mp ON pm.platform_id = mp.id
                 WHERE pm.model_name = ?1 AND pm.platform_id != ?2
                   AND pm.is_enabled = 1 AND mp.is_enabled = 1",
            )
            .map_err(|e| e.to_string())?;
        let rival_platforms: Vec<String> = rival_stmt
            .query_map(params![model_name, platform_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();

        // 只有真的存在同名竞争者时才去算赢家——否则这一栏是噪音。
        let winner_platform_id = if rival_platforms.is_empty() {
            None
        } else {
            crate::proxy::winning_platform_for_model(&db, &model_name)
        };

        // 最近一次真实失败。这些记录网关早就在写了（`log_failure`），
        // 只是模型中心一直没读——错误原因就摆在库里，界面却只有一个绿点。
        let (last_error, last_error_at) = conn
            .query_row(
                "SELECT error_message, timestamp FROM request_logs
                 WHERE is_error = 1 AND (model = ?1 OR model = ?2)
                 ORDER BY id DESC LIMIT 1",
                params![model_name, format!("{platform_id}:{model_name}")],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .ok()
            .map(|(message, at)| (Some(message), Some(at)))
            .unwrap_or((None, None));

        result.push(ModelRouting {
            model_id,
            model_name,
            rival_platforms,
            winner_platform_id,
            last_error,
            last_error_at,
        });
    }
    Ok(result)
}

/// 按前端给的顺序重写各平台的 `priority`（第一个最高）。
///
/// 路由用 `ORDER BY priority DESC, weight DESC` 在同名模型的多个平台之间决胜负，
/// 但这个字段以前在界面上完全不可编辑——全都停在默认 0，于是胜负实际由模型名的
/// 哈希决定：任意、不可见、不可控。列表顺序即优先级，拖一下就定了。
#[tauri::command]
pub fn reorder_platforms(
    db: State<'_, Arc<DbManager>>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let mut conn = db.get_connection().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let total = ordered_ids.len() as i32;
    for (index, id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE model_platforms SET priority = ?1 WHERE id = ?2",
            params![total - index as i32, id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
