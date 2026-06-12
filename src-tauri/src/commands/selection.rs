use tauri::{AppHandle, Emitter, Manager, State};
use std::sync::Arc;
use crate::db::DbManager;

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
    db.delete_selection_history_item(id).map_err(|e| e.to_string())
}

/// Clear all selection history.
#[tauri::command]
pub fn clear_selection_history(
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.clear_selection_history().map_err(|e| e.to_string())
}

// ── Translation Commands ──────────────────────────────

/// Translate text using LLM via the proxy gateway.
#[tauri::command]
pub async fn translate_text(
    text: String,
    target_lang: String,
    source_lang: Option<String>,
    chat_model: Option<String>,
    prompt: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<crate::selection::CaptureResult, String> {
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

    // Resolve model
    let model = chat_model.unwrap_or_else(|| {
        db.get_setting("target_model")
            .ok()
            .flatten()
            .unwrap_or_else(|| "deepseek-chat".to_string())
    });

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
    let port = db.get_setting("proxy_port")
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

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Translation API error {}: {}", status, body));
    }

    let chat_resp: ChatResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse translation response: {}", e))?;

    let translated = chat_resp
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();

    let detected = source_lang.unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Save to translation history
    let _ = db.add_translation_history(&text, &translated, &detected, &target_lang);

    Ok(crate::selection::CaptureResult {
        text: translated,
        source: detected,
        window_title: target_lang,
        process_name: "translation".to_string(),
        timestamp,
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

    let port = db.get_setting("proxy_port")
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
    let valid_codes = ["en-us", "zh-cn", "zh-tw", "ja-jp", "ko-kr", "fr-fr", "de-de", "it-it", "es-es", "pt-pt", "ru-ru", "pl-pl", "ar-sa", "tr-tr", "th-th", "vi-vn", "id-id", "ur-pk", "ms-my", "uk-ua"];
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
) -> Result<Vec<crate::selection::SelectionHistoryEntry>, String> {
    db.get_translation_history(limit).map_err(|e| e.to_string())
}

/// Delete a single translation history entry.
#[tauri::command]
pub fn delete_translation_history_item(
    id: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.delete_translation_history_item(id).map_err(|e| e.to_string())
}

/// Clear all translation history.
#[tauri::command]
pub fn clear_translation_history(
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.clear_translation_history().map_err(|e| e.to_string())
}

// ── Auto-Capture Commands ────────────────────────────────

/// Start or stop the auto-capture monitor.
/// When enabled, it polls UIA every ~500ms and emits `selection-auto-captured` events.
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
        // Start new monitor — poll every 500ms
        let h = crate::selection::start_auto_capture_monitor(app_handle.clone(), 500);
        *guard = Some(h);
        Ok(true)
    } else {
        // Stop monitor
        if let Some(h) = guard.take() {
            h.abort();
        }
        Ok(false)
    }
}
