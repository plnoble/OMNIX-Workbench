use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::agent::AgentManager;
use crate::db::DbManager;
use crate::runtime::{
    agent_definition, evaluate_model_compatibility, resolve_model_selection, AgentBinding, AgentId,
    AgentSessionConfig, AgentSessionRecord, ModelCompatibility, ModelCompatibilityLevel,
    ModelSelection, PermissionPolicy, RuntimeEvent, WorkMode,
};
use crate::runtime_manager::RuntimeManager;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeAgentCatalogEntry {
    pub id: String,
    pub name: String,
    pub status: String,
    pub runtime_status: String,
    pub installation_source: Option<String>,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub supports_structured_events: bool,
    pub supports_resume: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeModelOption {
    pub id: String,
    pub label: String,
    pub provider_name: Option<String>,
    pub provider_type: Option<String>,
    pub model_name: Option<String>,
    pub health_status: String,
    pub selection: ModelSelection,
    pub compatibility: ModelCompatibility,
    /// True for the option the Work page should pre-select (the Agent's bound
    /// model, or the global default model), so a user's configured default
    /// reaches the runtime instead of silently falling back to Agent default.
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRuntimeSessionRequest {
    pub conversation_id: String,
    pub agent: AgentId,
    pub workspace_path: String,
    pub model: ModelSelection,
    pub permission: PermissionPolicy,
    pub work_mode: WorkMode,
}

pub fn load_runtime_model_options(
    db: &DbManager,
    agent: AgentId,
) -> Result<Vec<RuntimeModelOption>, String> {
    let mut options = vec![RuntimeModelOption {
        id: "agent_default".into(),
        label: "Agent 官方默认".into(),
        provider_name: None,
        provider_type: None,
        model_name: None,
        health_status: "native".into(),
        selection: ModelSelection::AgentDefault,
        compatibility: ModelCompatibility {
            level: ModelCompatibilityLevel::Native,
            selectable: true,
            reason: "使用 Agent 自身配置和官方默认模型".into(),
        },
        is_default: false,
    }];
    let builtins: &[&str] = match agent {
        AgentId::ClaudeCode => &["sonnet", "opus", "haiku"],
        AgentId::Codex => &["gpt-5-codex"],
    };
    options.extend(builtins.iter().map(|model_name| RuntimeModelOption {
        id: format!("builtin:{model_name}"),
        label: format!("{model_name} · Agent 官方"),
        provider_name: None,
        provider_type: None,
        model_name: Some((*model_name).to_string()),
        health_status: "native".into(),
        selection: ModelSelection::Builtin {
            model_name: (*model_name).to_string(),
        },
        compatibility: ModelCompatibility {
            level: ModelCompatibilityLevel::Native,
            selectable: true,
            reason: "由 Agent 官方连接直接使用".into(),
        },
        is_default: false,
    }));

    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT pm.id, pm.platform_id, pm.model_name, pm.status,
                    mp.name, mp.api_type
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.is_enabled = 1 AND mp.is_enabled = 1
             ORDER BY mp.name, pm.model_name",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (id, platform_id, model_name, status, provider_name, provider_type) =
            row.map_err(|error| error.to_string())?;
        options.push(RuntimeModelOption {
            id,
            label: format!("{model_name} · {provider_name}"),
            provider_name: Some(provider_name),
            provider_type: Some(provider_type.clone()),
            model_name: Some(model_name.clone()),
            health_status: status.clone(),
            selection: ModelSelection::Omnix {
                platform_id,
                model_name,
            },
            compatibility: evaluate_model_compatibility(agent, &provider_type, &status),
            is_default: false,
        });
    }

    // Mark the option the Work page should pre-select: the Agent's bound model
    // (or the global default model) so the user's configured default reaches the
    // runtime instead of "Agent 官方默认" (which sends model: null to Codex).
    let default_selection =
        resolve_default_model_selection(db, agent).unwrap_or(ModelSelection::AgentDefault);
    if let Some(option) = options
        .iter_mut()
        .find(|option| option.selection == default_selection && option.compatibility.selectable)
    {
        option.is_default = true;
    } else if let Some(option) = options.first_mut() {
        option.is_default = true;
    }

    Ok(options)
}

/// Read the global default model setting (`default_model = "platform_id:model_name"`),
/// returning an OMNIX selection only when the model is still enabled.
fn load_global_default_model(db: &DbManager) -> Result<Option<ModelSelection>, String> {
    let Some(raw) = db
        .get_setting("default_model")
        .map_err(|error| error.to_string())?
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let Some((platform_id, model_name)) = raw.split_once(':') else {
        return Ok(None);
    };
    let platform_id = platform_id.trim().to_string();
    let model_name = model_name.trim().to_string();
    if platform_id.is_empty() || model_name.is_empty() {
        return Ok(None);
    }
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.platform_id = ?1 AND pm.model_name = ?2
               AND pm.is_enabled = 1 AND mp.is_enabled = 1",
            params![platform_id, model_name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Ok(None);
    }
    Ok(Some(ModelSelection::Omnix {
        platform_id,
        model_name,
    }))
}

/// Resolve the model an Agent should use when no explicit session model is set:
/// the Agent's binding, then the global default model, then Agent default.
fn resolve_default_model_selection(
    db: &DbManager,
    agent: AgentId,
) -> Result<ModelSelection, String> {
    let resolved = resolve_model_selection(None, load_agent_binding(db, agent)?);
    if resolved == ModelSelection::AgentDefault {
        Ok(load_global_default_model(db)?.unwrap_or(ModelSelection::AgentDefault))
    } else {
        Ok(resolved)
    }
}

fn load_agent_binding(db: &DbManager, agent: AgentId) -> Result<Option<AgentBinding>, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let row: Option<(String, Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT COALESCE(binding_kind, 'omnix'), builtin_model, platform_id, model_name
             FROM agent_platform_bindings
             WHERE agent_name = ?1 AND enabled = 1
             LIMIT 1",
            params![agent.display_name()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(
        row.and_then(|(kind, builtin, platform, model)| match kind.as_str() {
            "default" => Some(AgentBinding::Default),
            "builtin" => builtin.map(|model_name| AgentBinding::Builtin { model_name }),
            "omnix" => platform
                .zip(model)
                .map(|(platform_id, model_name)| AgentBinding::Omnix {
                    platform_id,
                    model_name,
                }),
            _ => None,
        }),
    )
}

fn validate_runtime_model(
    db: &DbManager,
    agent: AgentId,
    selection: &ModelSelection,
) -> Result<(), String> {
    let ModelSelection::Omnix {
        platform_id,
        model_name,
    } = selection
    else {
        return Ok(());
    };
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let model: Option<(String, String)> = conn
        .query_row(
            "SELECT mp.api_type, pm.status
             FROM platform_models pm
             JOIN model_platforms mp ON pm.platform_id = mp.id
             WHERE pm.platform_id = ?1 AND pm.model_name = ?2
               AND pm.is_enabled = 1 AND mp.is_enabled = 1",
            params![platform_id, model_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (provider_type, status) =
        model.ok_or_else(|| format!("模型未启用或不存在: {platform_id}:{model_name}"))?;
    let compatibility = evaluate_model_compatibility(agent, &provider_type, &status);
    if compatibility.selectable {
        Ok(())
    } else {
        Err(compatibility.reason)
    }
}

#[tauri::command]
pub fn runtime_get_agent_catalog(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<Vec<RuntimeAgentCatalogEntry>, String> {
    let detected = agent_manager.detect_agents();
    let entries = [
        ("claude_code", "Claude Code", true),
        ("codex", "Codex", true),
        ("gemini_cli", "Gemini CLI", false),
        ("opencode", "OpenCode", false),
    ]
    .into_iter()
    .map(|(id, name, supported)| {
        let installation = detected.iter().find(|candidate| candidate.name == name);
        let path = installation
            .filter(|candidate| candidate.status == "installed")
            .map(|candidate| candidate.path.clone());
        let source = path.as_deref().map(|path| {
            let normalized = path.replace('\\', "/").to_lowercase();
            if normalized.contains("/.omnix/agents/") {
                "managed".to_string()
            } else {
                "system".to_string()
            }
        });
        RuntimeAgentCatalogEntry {
            id: id.into(),
            name: name.into(),
            status: installation
                .map(|candidate| candidate.status.clone())
                .unwrap_or_else(|| "not_installed".into()),
            runtime_status: if supported { "supported" } else { "pending" }.into(),
            installation_source: source,
            executable_path: path,
            version: installation
                .filter(|candidate| !candidate.version.is_empty())
                .map(|candidate| candidate.version.clone()),
            supports_structured_events: supported,
            supports_resume: supported,
            detail: if supported {
                format!("{} 结构化运行适配器", name)
            } else {
                "待完成结构化事件和恢复协议适配".into()
            },
        }
    })
    .collect();
    Ok(entries)
}

#[tauri::command]
pub fn runtime_get_model_options(
    agent: AgentId,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<RuntimeModelOption>, String> {
    load_runtime_model_options(&db, agent)
}

#[tauri::command]
pub async fn runtime_start_session(
    request: CreateRuntimeSessionRequest,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
    agent_manager: State<'_, Arc<AgentManager>>,
    db: State<'_, Arc<DbManager>>,
) -> Result<AgentSessionRecord, String> {
    if request.workspace_path.trim().is_empty() {
        return Err("请选择工作区后再开始工作".into());
    }
    let workspace_path = if request.workspace_path == "direct" {
        dirs::home_dir()
            .ok_or_else(|| "无法确定当前用户目录".to_string())?
            .to_string_lossy()
            .into_owned()
    } else {
        request.workspace_path
    };
    let definition = agent_definition(request.agent);
    let executable_path = agent_manager
        .find_agent_path(definition.display_name)
        .ok_or_else(|| format!("{} 尚未安装或未被检测到", definition.display_name))?;
    let selected_model = if request.model == ModelSelection::AgentDefault {
        resolve_default_model_selection(&db, request.agent)?
    } else {
        request.model
    };
    validate_runtime_model(&db, request.agent, &selected_model)?;
    runtime_manager
        .start_session(AgentSessionConfig {
            conversation_id: request.conversation_id,
            agent: request.agent,
            executable_path,
            workspace_path,
            model: selected_model,
            permission: request.permission,
            work_mode: request.work_mode,
        })
        .await
}

#[tauri::command]
pub async fn runtime_send_message(
    session_id: String,
    prompt: String,
    display_text: Option<String>,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
) -> Result<(), String> {
    runtime_manager
        .send_message_with_display(
            &session_id,
            &prompt,
            display_text.as_deref().unwrap_or(&prompt),
        )
        .await
}

#[tauri::command]
pub async fn runtime_respond_approval(
    session_id: String,
    request_id: String,
    approved: bool,
    for_session: bool,
    approval_method: String,
    requested_permissions: Option<serde_json::Value>,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
) -> Result<(), String> {
    runtime_manager
        .respond_approval(
            &session_id,
            &request_id,
            approved,
            for_session,
            &approval_method,
            requested_permissions,
        )
        .await
}

#[tauri::command]
pub async fn runtime_stop_session(
    session_id: String,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
) -> Result<(), String> {
    runtime_manager.stop_session(&session_id).await
}

#[tauri::command]
pub async fn runtime_resume_session(
    session_id: String,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
) -> Result<AgentSessionRecord, String> {
    runtime_manager.resume_session(&session_id).await
}

#[tauri::command]
pub fn runtime_get_session(
    session_id: String,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
) -> Result<AgentSessionRecord, String> {
    runtime_manager.get_session(&session_id)
}

#[tauri::command]
pub fn runtime_get_events(
    session_id: String,
    runtime_manager: State<'_, Arc<RuntimeManager>>,
) -> Result<Vec<RuntimeEvent>, String> {
    runtime_manager.list_events(&session_id)
}

#[tauri::command]
pub fn runtime_list_conversation_sessions(
    conversation_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<AgentSessionRecord>, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare("SELECT id FROM agent_sessions WHERE conversation_id = ?1 ORDER BY created_at ASC")
        .map_err(|error| error.to_string())?;
    let ids = statement
        .query_map(params![conversation_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    ids.map(|id| {
        let id = id.map_err(|error| error.to_string())?;
        crate::runtime::get_agent_session_record(&db, &id)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use crate::db::DbManager;
    use crate::runtime::{AgentId, ModelCompatibilityLevel, ModelSelection};

    use super::load_runtime_model_options;

    #[test]
    fn model_catalog_keeps_incompatible_models_visible_with_reason() {
        let db_path = std::env::temp_dir().join(format!(
            "omnix_runtime_models_{}.sqlite",
            chrono::Utc::now().timestamp_micros()
        ));
        let db = DbManager::new_runtime_test(db_path.clone());
        let conn = db.get_connection().expect("db connection");
        conn.execute_batch(
            "CREATE TABLE model_platforms (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, api_type TEXT NOT NULL,
                api_key TEXT NOT NULL DEFAULT '', api_address TEXT NOT NULL DEFAULT '',
                is_enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE platform_models (
                id TEXT PRIMARY KEY, platform_id TEXT NOT NULL, model_name TEXT NOT NULL,
                is_enabled INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'success'
            );
            INSERT INTO model_platforms (id, name, api_type) VALUES
                ('responses', 'Responses Provider', 'openai-response'),
                ('chat', 'Chat Provider', 'openai-compatible'),
                ('claude', 'Claude Provider', 'anthropic');
            INSERT INTO platform_models (id, platform_id, model_name, status) VALUES
                ('responses:gpt', 'responses', 'gpt-real', 'success'),
                ('chat:gpt', 'chat', 'gpt-chat-only', 'success'),
                ('claude:c', 'claude', 'claude-only', 'success');",
        )
        .expect("model catalog fixture");
        drop(conn);

        let models = load_runtime_model_options(&db, AgentId::Codex).expect("model options");
        let responses = models
            .iter()
            .find(|model| model.model_name.as_deref() == Some("gpt-real"))
            .expect("Responses model");
        let chat = models
            .iter()
            .find(|model| model.model_name.as_deref() == Some("gpt-chat-only"))
            .expect("chat model");
        let unsupported = models
            .iter()
            .find(|model| model.model_name.as_deref() == Some("claude-only"))
            .expect("unsupported model");

        assert!(responses.compatibility.selectable);
        assert_eq!(
            responses.compatibility.level,
            ModelCompatibilityLevel::Gateway
        );
        // Chat-Completions providers are now selectable via the translating gateway.
        assert!(chat.compatibility.selectable);
        assert_eq!(chat.compatibility.level, ModelCompatibilityLevel::Gateway);
        // A provider Codex cannot use at all stays visible but unselectable.
        assert!(!unsupported.compatibility.selectable);
        assert_eq!(
            unsupported.compatibility.level,
            ModelCompatibilityLevel::Unsupported
        );

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn bound_codex_model_is_marked_as_default_option() {
        let db_path = std::env::temp_dir().join(format!(
            "omnix_runtime_default_{}.sqlite",
            chrono::Utc::now().timestamp_micros()
        ));
        let db = DbManager::new_runtime_test(db_path.clone());
        let conn = db.get_connection().expect("db connection");
        conn.execute_batch(
            "CREATE TABLE model_platforms (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, api_type TEXT NOT NULL,
                api_key TEXT NOT NULL DEFAULT '', api_address TEXT NOT NULL DEFAULT '',
                is_enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE platform_models (
                id TEXT PRIMARY KEY, platform_id TEXT NOT NULL, model_name TEXT NOT NULL,
                is_enabled INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'success'
            );
            CREATE TABLE agent_platform_bindings (
                agent_name TEXT PRIMARY KEY, platform_id TEXT, model_name TEXT,
                binding_kind TEXT DEFAULT 'omnix', builtin_model TEXT,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO model_platforms (id, name, api_type) VALUES
                ('volcano', 'Volcano', 'openai-compatible');
            INSERT INTO platform_models (id, platform_id, model_name) VALUES
                ('volcano:doubao', 'volcano', 'doubao-pro');
            INSERT INTO agent_platform_bindings (agent_name, platform_id, model_name, binding_kind)
                VALUES ('Codex', 'volcano', 'doubao-pro', 'omnix');",
        )
        .expect("binding fixture");
        drop(conn);

        let models = load_runtime_model_options(&db, AgentId::Codex).expect("model options");
        let default_option = models
            .iter()
            .find(|model| model.is_default)
            .expect("a default option is marked");
        assert_eq!(
            default_option.selection,
            ModelSelection::Omnix {
                platform_id: "volcano".into(),
                model_name: "doubao-pro".into(),
            }
        );
        assert!(default_option.compatibility.selectable);
        // The "Agent 官方默认" option must not be the default when a binding exists.
        let agent_default = models
            .iter()
            .find(|model| model.selection == ModelSelection::AgentDefault)
            .expect("agent default option");
        assert!(!agent_default.is_default);

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }
}
