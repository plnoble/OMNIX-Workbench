use crate::db::DbManager;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

// ── Selection Assistant Commands ───────────────────────

/// Capture selected text from the currently focused application
/// using the hybrid UIA + clipboard approach, then show the Quick Assistant.
#[tauri::command]
pub async fn capture_selection_and_show(app_handle: AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let result = crate::selection::capture_selection_with_context().await?;

    if result.text.trim().is_empty() {
        return Err("No text captured".to_string());
    }

    // Save to selection history
    if let Some(db) = app_handle.try_state::<Arc<crate::db::DbManager>>() {
        let blacklist = db
            .get_setting("selection_assistant_blacklist")
            .ok()
            .flatten()
            .unwrap_or_else(|| "[]".into());
        if crate::selection::is_capture_blocked(
            &blacklist,
            &result.process_name,
            &result.window_title,
        ) {
            return Err("当前应用位于快捷助手黑名单中".into());
        }
        let _ = crate::db::DbManager::add_selection_history(
            &db,
            &result.text,
            &result.source,
            &result.window_title,
            &result.process_name,
        );
    }

    // Show the Quick Assistant window with the captured text
    if let Some(qa) = app_handle.get_webview_window("quick-assistant") {
        let _ = qa.show();
        let _ = qa.set_focus();
        let _ = app_handle.emit("qa-preset-text", result.text);
    } else {
        return Err("Quick Assistant window not found".to_string());
    }

    Ok(())
}

/// Capture selected text only (without showing the assistant).
/// Useful for testing and settings UI.
#[tauri::command]
pub async fn get_selection_text() -> Result<String, String> {
    crate::selection::capture_selection().await
}

/// Capture selected text with context (window title, process name).
#[tauri::command]
pub async fn get_selection_with_context() -> Result<crate::selection::CaptureResult, String> {
    crate::selection::capture_selection_with_context().await
}

/// Get selection history entries.
#[tauri::command]
pub fn get_selection_history(
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<crate::selection::SelectionHistoryEntry>, String> {
    db.get_selection_history(limit).map_err(|e| e.to_string())
}

/// Delete a single selection history entry.
#[tauri::command]
pub fn delete_selection_history_item(
    id: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.delete_selection_history_item(id)
        .map_err(|e| e.to_string())
}

/// Clear all selection history.
#[tauri::command]
pub fn clear_selection_history(db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    db.clear_selection_history().map_err(|e| e.to_string())
}

// ── Translation Commands ──────────────────────────────

/// The first enabled platform model that has an API key, as `platform_id:model_name`
/// (the format the gateway router resolves). Used as a last-resort default so
/// translation works whenever ANY chat provider is configured.
fn first_enabled_chat_model(db: &DbManager) -> Option<String> {
    let conn = db.get_connection().ok()?;
    conn.query_row(
        "SELECT pm.platform_id, pm.model_name FROM platform_models pm
         JOIN model_platforms mp ON pm.platform_id = mp.id
         WHERE pm.is_enabled = 1 AND mp.is_enabled = 1
           AND (TRIM(mp.api_key) != '' OR mp.api_type = 'ollama')
         ORDER BY mp.priority DESC, mp.weight DESC
         LIMIT 1",
        [],
        |r| Ok(format!("{}:{}", r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )
    .ok()
}

/// Translate text using LLM via the proxy gateway.
#[tauri::command]
pub async fn translate_text(
    text: String,
    target_lang: String,
    source_lang: Option<String>,
    chat_model: Option<String>,
    prompt: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<crate::selection::TranslateResult, String> {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct ChatRequest {
        model: String,
        messages: Vec<ChatMessage>,
        stream: bool,
    }

    #[derive(Serialize, Deserialize)]
    struct ChatMessage {
        role: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<Choice>,
    }

    #[derive(Deserialize)]
    struct Choice {
        message: ChatMessage,
    }

    // Resolve a definitely-routable chat model: explicit → target_model (内置功能
    // 默认模型) → the first enabled platform model that has an API key. Erroring
    // here (instead of defaulting to an unconfigured "deepseek-chat") makes the
    // real cause visible when nothing is configured.
    let model = chat_model
        .filter(|m| !m.trim().is_empty())
        .or_else(|| {
            db.get_setting("target_model")
                .ok()
                .flatten()
                .filter(|m| !m.trim().is_empty())
        })
        .or_else(|| first_enabled_chat_model(&db))
        .ok_or_else(|| {
            "未配置可用的聊天模型。请到「模型」启用一个带 API Key 的供应商，或在「设置 → 内置功能默认模型」选择一个。".to_string()
        })?;
    let model_label = model.clone();

    // Resolve prompt template
    let default_prompt = std::include_str!("../../translate_prompt_default.txt").to_string();
    let prompt_template = prompt.unwrap_or(default_prompt);

    let target_lang_name = match target_lang.as_str() {
        "zh-cn" => "Chinese (Simplified)",
        "zh-tw" => "Chinese (Traditional)",
        "en-us" => "English",
        "ja-jp" => "Japanese",
        "ko-kr" => "Korean",
        "fr-fr" => "French",
        "de-de" => "German",
        "es-es" => "Spanish",
        "ru-ru" => "Russian",
        other => other,
    };

    let final_prompt = prompt_template
        .replace("{{target_language}}", target_lang_name)
        .replace("{{text}}", &text);

    // Resolve proxy port
    let port = db
        .get_setting("proxy_port")
        .ok()
        .flatten()
        .unwrap_or_else(|| "1421".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", port);

    let request = ChatRequest {
        model,
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: final_prompt,
        }],
        stream: false,
    };

    let response = client
        .post(&url)
        .json(&request)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| format!("Translation request failed: {}", e))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let snippet = |s: &str| s.chars().take(300).collect::<String>();
    if !status.is_success() {
        return Err(format!("翻译接口错误 {} (模型 {}): {}", status, model_label, snippet(&body)));
    }

    let chat_resp: ChatResponse = serde_json::from_str(&body)
        .map_err(|e| format!("解析翻译响应失败: {} — 原始响应: {}", e, snippet(&body)))?;

    let translated = chat_resp
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();

    // A blank result almost always means the chosen model is unavailable — make
    // that visible instead of silently returning empty text.
    if translated.trim().is_empty() {
        return Err(format!(
            "模型 '{}' 返回了空译文，可能不可用或不支持该请求。原始响应: {}",
            model_label,
            snippet(&body)
        ));
    }

    let detected = source_lang.unwrap_or_else(|| "unknown".to_string());

    // Save to translation history
    let _ = db.add_translation_history(&text, &translated, &detected, &target_lang);

    Ok(crate::selection::TranslateResult {
        translated_text: translated,
        detected_lang: detected,
        target_lang,
    })
}

/// Detect the language of the given text using LLM.
#[tauri::command]
pub async fn detect_language(
    text: String,
    chat_model: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct ChatRequest {
        model: String,
        messages: Vec<ChatMessage>,
        stream: bool,
    }

    #[derive(Serialize, Deserialize)]
    struct ChatMessage {
        role: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<Choice>,
    }

    #[derive(Deserialize)]
    struct Choice {
        message: ChatMessage,
    }

    let model = chat_model.unwrap_or_else(|| {
        db.get_setting("target_model")
            .ok()
            .flatten()
            .unwrap_or_else(|| "deepseek-chat".to_string())
    });

    let lang_list = "en-us, zh-cn, zh-tw, ja-jp, ko-kr, fr-fr, de-de, it-it, es-es, pt-pt, ru-ru, pl-pl, ar-sa, tr-tr, th-th, vi-vn, id-id, ur-pk, ms-my, uk-ua";

    let prompt = format!(
        "Identify the language of the text below. Output ONLY the language code from this list: {}. If unknown, output \"unknown\".\n\n<text>\n{}\n</text>",
        lang_list,
        text.chars().take(500).collect::<String>()  // Truncate to 500 chars for detection
    );

    let port = db
        .get_setting("proxy_port")
        .ok()
        .flatten()
        .unwrap_or_else(|| "1421".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", port);

    let request = ChatRequest {
        model,
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }],
        stream: false,
    };

    let response = client
        .post(&url)
        .json(&request)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Language detection request failed: {}", e))?;

    if !response.status().is_success() {
        return Ok("unknown".to_string());
    }

    let chat_resp: ChatResponse = response
        .json()
        .await
        .map_err(|_| "Failed to parse detection response".to_string())?;

    let detected = chat_resp
        .choices
        .first()
        .map(|c| c.message.content.trim().to_lowercase())
        .unwrap_or_default();

    // Validate the detected language is in our list
    let valid_codes = [
        "en-us", "zh-cn", "zh-tw", "ja-jp", "ko-kr", "fr-fr", "de-de", "it-it", "es-es", "pt-pt",
        "ru-ru", "pl-pl", "ar-sa", "tr-tr", "th-th", "vi-vn", "id-id", "ur-pk", "ms-my", "uk-ua",
    ];
    if valid_codes.contains(&detected.as_str()) {
        Ok(detected)
    } else {
        Ok("unknown".to_string())
    }
}

/// Get translation history entries.
#[tauri::command]
pub fn get_translation_history(
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<crate::selection::TranslateHistoryEntry>, String> {
    db.get_translation_history(limit).map_err(|e| e.to_string())
}

/// Delete a single translation history entry.
#[tauri::command]
pub fn delete_translation_history_item(
    id: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.delete_translation_history_item(id)
        .map_err(|e| e.to_string())
}

/// Clear all translation history.
#[tauri::command]
pub fn clear_translation_history(db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    db.clear_translation_history().map_err(|e| e.to_string())
}

// ── Auto-Capture Commands ────────────────────────────────

/// Start or stop the selection monitor.
/// When enabled, it watches the mouse button and, on release with a non-empty
/// selection, shows the popup once (no focus steal) and emits `selection-auto-captured`.
#[tauri::command]
pub async fn toggle_selection_auto_capture(
    app_handle: AppHandle,
    enabled: bool,
) -> Result<bool, String> {
    use std::sync::Mutex as StdMutex;

    struct AutoCaptureHandle(StdMutex<Option<tokio::task::JoinHandle<()>>>);

    let handle = app_handle.try_state::<AutoCaptureHandle>();
    if handle.is_none() {
        app_handle.manage(AutoCaptureHandle(StdMutex::new(None)));
    }
    let handle = app_handle.state::<AutoCaptureHandle>();

    let mut guard = handle.0.lock().map_err(|e| format!("Lock error: {}", e))?;

    if enabled {
        // Stop existing monitor if any
        if let Some(h) = guard.take() {
            h.abort();
        }
        // Start new monitor — watch the mouse button at ~60ms (cheap, no UIA).
        let h = crate::selection::start_auto_capture_monitor(app_handle.clone(), 60);
        *guard = Some(h);
        Ok(true)
    } else {
        // Stop monitor
        if let Some(h) = guard.take() {
            h.abort();
        }
        // Also hide any popup left on screen, so disabling fully closes it.
        if let Some(qa) = app_handle.get_webview_window("quick-assistant") {
            let _ = qa.hide();
        }
        Ok(false)
    }
}
