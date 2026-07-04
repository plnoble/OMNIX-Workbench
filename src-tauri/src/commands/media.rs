//! Media generation commands — the I/O layer over `crate::media`.
//!
//! Owns HTTP calls to the provider, file storage under `~/.omnix/media/`, and
//! the `media_tasks` table. Protocol shapes live in `crate::media` (pure,
//! unit-tested); this file stays thin per the BORROWINGS 分层约定.

use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::DbManager;
use crate::media::{
    build_image_request, build_video_create_request, build_video_poll_url,
    parse_image_response, parse_video_create_response, parse_video_poll_response,
    suggested_models, ImageOutput, MediaProviderKind, MediaTaskStatus, VideoRequest,
};

/// `~/.omnix/media/` — generated artifacts live here, only paths go in the DB.
pub(crate) fn media_dir() -> Result<PathBuf, String> {
    let mut dir = dirs::home_dir().ok_or_else(|| "无法确定用户目录".to_string())?;
    dir.push(".omnix");
    dir.push("media");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

/// `~/.omnix/media/attachments/` — chat image attachments (vision input).
pub(crate) fn attachments_dir() -> Result<PathBuf, String> {
    let mut dir = media_dir()?;
    dir.push("attachments");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

/// Reads a chat attachment as a data URL for bubble thumbnails. Only serves
/// files inside the managed attachments directory.
#[tauri::command]
pub fn media_read_attachment(path: String) -> Result<String, String> {
    let root = attachments_dir()?;
    let target = PathBuf::from(&path);
    if !target.starts_with(&root) {
        return Err("附件路径越界".into());
    }
    let bytes = std::fs::read(&target).map_err(|error| error.to_string())?;
    let mime = match target.extension().and_then(|ext| ext.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// A media task row as the frontend sees it.
#[derive(Debug, Clone, Serialize)]
pub struct MediaTask {
    pub id: String,
    pub platform_id: String,
    pub kind: String,
    pub model: String,
    pub prompt: String,
    pub params_json: String,
    pub status: String,
    pub progress: i64,
    pub external_id: Option<String>,
    pub result_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
}

/// Suggested media models for the Studio dropdowns.
#[derive(Debug, Clone, Serialize)]
pub struct MediaModelSuggestions {
    pub image: Vec<String>,
    pub video: Vec<String>,
}

#[tauri::command]
pub fn media_model_suggestions() -> MediaModelSuggestions {
    MediaModelSuggestions {
        image: suggested_models(MediaProviderKind::OpenAiImages)
            .iter()
            .map(|model| model.to_string())
            .collect(),
        video: suggested_models(MediaProviderKind::AgnesVideo)
            .iter()
            .map(|model| model.to_string())
            .collect(),
    }
}

/// Resolves an enabled platform's decrypted key + base address (same pattern
/// as `knowledge::resolve_embedding_platform` / proxy key handling).
pub(crate) fn resolve_media_platform(
    db: &DbManager,
    platform_id: &str,
) -> Result<(String, String), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT api_key, api_address FROM model_platforms WHERE id = ?1 AND is_enabled = 1",
            params![platform_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (api_key, api_address) =
        row.ok_or_else(|| format!("平台未启用或不存在: {platform_id}"))?;
    if api_address.trim().is_empty() {
        return Err(format!("平台 {platform_id} 未配置 API 地址"));
    }
    Ok((crate::crypto::decrypt(&api_key), api_address))
}

pub(crate) fn insert_task(
    db: &DbManager,
    task: &MediaTask,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO media_tasks (id, platform_id, kind, model, prompt, params_json, status, progress, external_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            task.id,
            task.platform_id,
            task.kind,
            task.model,
            task.prompt,
            task.params_json,
            task.status,
            task.progress,
            task.external_id,
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Updates a task's lifecycle fields; `None` leaves a column unchanged.
pub(crate) fn update_task(
    db: &DbManager,
    task_id: &str,
    status: MediaTaskStatus,
    progress: Option<i64>,
    result_path: Option<&str>,
    raw_response: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE media_tasks SET
            status = ?2,
            progress = COALESCE(?3, progress),
            result_path = COALESCE(?4, result_path),
            raw_response = COALESCE(?5, raw_response),
            error = ?6,
            updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
        params![task_id, status.as_str(), progress, result_path, raw_response, error],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_task(db: &DbManager, task_id: &str) -> Result<MediaTask, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id, platform_id, kind, model, prompt, params_json, status, progress,
                external_id, result_path, error, created_at
         FROM media_tasks WHERE id = ?1",
        params![task_id],
        map_task_row,
    )
    .map_err(|error| format!("任务不存在: {error}"))
}

fn map_task_row(row: &rusqlite::Row) -> rusqlite::Result<MediaTask> {
    Ok(MediaTask {
        id: row.get(0)?,
        platform_id: row.get(1)?,
        kind: row.get(2)?,
        model: row.get(3)?,
        prompt: row.get(4)?,
        params_json: row.get(5)?,
        status: row.get(6)?,
        progress: row.get(7)?,
        external_id: row.get(8)?,
        result_path: row.get(9)?,
        error: row.get(10)?,
        created_at: row.get(11)?,
    })
}

#[tauri::command]
pub fn media_list_tasks(db: State<'_, Arc<DbManager>>) -> Result<Vec<MediaTask>, String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT id, platform_id, kind, model, prompt, params_json, status, progress,
                    external_id, result_path, error, created_at
             FROM media_tasks ORDER BY created_at DESC, id DESC LIMIT 200",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], map_task_row)
        .map_err(|error| error.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Deletes a task row AND its artifact file (gallery delete = disk delete).
#[tauri::command]
pub fn media_delete_task(task_id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let task = get_task(&db, &task_id)?;
    if let Some(path) = task.result_path.as_deref() {
        // Only ever delete inside the managed media dir.
        let media_root = media_dir()?;
        let target = PathBuf::from(path);
        if target.starts_with(&media_root) {
            let _ = std::fs::remove_file(&target);
        }
    }
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM media_tasks WHERE id = ?1", params![task_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Returns a task's artifact as a data URL for `<img>` rendering (same pattern
/// as FilePreviewPanel). Only serves files inside `~/.omnix/media/`.
#[tauri::command]
pub fn media_read_file(task_id: String, db: State<'_, Arc<DbManager>>) -> Result<String, String> {
    let task = get_task(&db, &task_id)?;
    let path = task
        .result_path
        .ok_or_else(|| "任务尚无产物文件".to_string())?;
    let media_root = media_dir()?;
    let target = PathBuf::from(&path);
    if !target.starts_with(&media_root) {
        return Err("产物路径越界".into());
    }
    let bytes = std::fs::read(&target).map_err(|error| error.to_string())?;
    let mime = match target.extension().and_then(|ext| ext.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Generates an image synchronously: call the provider, save the artifact to
/// `~/.omnix/media/`, and record the task. Errors land on the task row (and
/// are returned) so the gallery shows failures too.
#[tauri::command]
pub async fn media_generate_image(
    platform_id: String,
    model: String,
    prompt: String,
    size: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<MediaTask, String> {
    if prompt.trim().is_empty() {
        return Err("提示词不能为空".into());
    }
    let (api_key, api_address) = resolve_media_platform(&db, &platform_id)?;

    let task_id = format!("media_{}", chrono::Utc::now().timestamp_micros());
    let task = MediaTask {
        id: task_id.clone(),
        platform_id: platform_id.clone(),
        kind: "image".into(),
        model: model.clone(),
        prompt: prompt.clone(),
        params_json: serde_json::json!({ "size": size }).to_string(),
        status: MediaTaskStatus::Running.as_str().into(),
        progress: 0,
        external_id: None,
        result_path: None,
        error: None,
        created_at: String::new(),
    };
    insert_task(&db, &task)?;

    match generate_image_inner(&api_key, &api_address, &model, &prompt, &size, &task_id).await {
        Ok((path, raw)) => {
            update_task(
                &db,
                &task_id,
                MediaTaskStatus::Completed,
                Some(100),
                Some(&path),
                Some(&raw),
                None,
            )?;
            get_task(&db, &task_id)
        }
        Err(error) => {
            let _ = update_task(
                &db,
                &task_id,
                MediaTaskStatus::Failed,
                None,
                None,
                None,
                Some(&error),
            );
            Err(error)
        }
    }
}

async fn generate_image_inner(
    api_key: &str,
    api_address: &str,
    model: &str,
    prompt: &str,
    size: &str,
    task_id: &str,
) -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/images/generations", api_address.trim_end_matches('/'));
    let body = build_image_request(model, prompt, size);
    let response = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("请求生图接口失败: {error}"))?;
    let status = response.status();
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("生图响应不是 JSON（HTTP {status}）: {error}"))?;
    let raw = truncate_raw(&payload);

    let output = parse_image_response(&payload).map_err(|error| {
        if status.is_success() {
            error
        } else {
            format!("HTTP {status}: {error}")
        }
    })?;
    let bytes = match output {
        ImageOutput::Base64(b64) => base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|error| format!("图片 base64 解码失败: {error}"))?,
        ImageOutput::Url(image_url) => client
            .get(&image_url)
            .send()
            .await
            .map_err(|error| format!("下载图片失败: {error}"))?
            .bytes()
            .await
            .map_err(|error| format!("读取图片内容失败: {error}"))?
            .to_vec(),
    };

    let mut path = media_dir()?;
    path.push(format!("{task_id}.png"));
    std::fs::write(&path, &bytes).map_err(|error| format!("保存图片失败: {error}"))?;
    Ok((path.to_string_lossy().into_owned(), raw))
}

/// Keeps the persisted raw response bounded (diagnostics, not an archive).
pub(crate) fn truncate_raw(value: &serde_json::Value) -> String {
    let mut raw = value.to_string();
    if raw.len() > 4000 {
        raw.truncate(4000);
    }
    raw
}

// ───────────────────────── video (async tasks) ─────────────────────────

/// Creates an async video-generation task with the provider and records it;
/// the background poller drives it to completion. For image-to-video, pass a
/// completed image task's id — its file is inlined as a data URL (whether the
/// provider accepts data URLs is verified live; a rejection surfaces on the
/// task card).
#[tauri::command]
pub async fn media_create_video_task(
    platform_id: String,
    model: String,
    prompt: String,
    width: u32,
    height: u32,
    num_frames: u32,
    frame_rate: u32,
    image_task_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<MediaTask, String> {
    if prompt.trim().is_empty() {
        return Err("提示词不能为空".into());
    }
    let (api_key, api_address) = resolve_media_platform(&db, &platform_id)?;

    // Inline the source image (图生视频), if any.
    let image_data_url = match image_task_id.as_deref() {
        Some(source_id) => {
            let source = get_task(&db, source_id)?;
            let path = source
                .result_path
                .ok_or_else(|| "所选图片任务尚无产物".to_string())?;
            let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ))
        }
        None => None,
    };

    let body = build_video_create_request(&VideoRequest {
        model: &model,
        prompt: &prompt,
        image: image_data_url.as_deref(),
        width,
        height,
        num_frames,
        frame_rate,
    });

    let task_id = format!("media_{}", chrono::Utc::now().timestamp_micros());
    let task = MediaTask {
        id: task_id.clone(),
        platform_id: platform_id.clone(),
        kind: "video".into(),
        model: model.clone(),
        prompt: prompt.clone(),
        params_json: serde_json::json!({
            "width": width, "height": height,
            "num_frames": num_frames, "frame_rate": frame_rate,
            "image_task_id": image_task_id,
        })
        .to_string(),
        status: MediaTaskStatus::Pending.as_str().into(),
        progress: 0,
        external_id: None,
        result_path: None,
        error: None,
        created_at: String::new(),
    };
    insert_task(&db, &task)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/videos", api_address.trim_end_matches('/'));
    let response = client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("提交视频任务失败: {error}"))?;
    let status = response.status();
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("视频任务响应不是 JSON（HTTP {status}）: {error}"))?;
    let raw = truncate_raw(&payload);

    match parse_video_create_response(&payload) {
        Ok(external_id) => {
            let conn = db.get_connection().map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE media_tasks SET external_id = ?2, raw_response = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                params![task_id, external_id, raw],
            )
            .map_err(|error| error.to_string())?;
            drop(conn);
            get_task(&db, &task_id)
        }
        Err(error) => {
            let message = if status.is_success() {
                error
            } else {
                format!("HTTP {status}: {error}")
            };
            let _ = update_task(
                &db,
                &task_id,
                MediaTaskStatus::Failed,
                None,
                None,
                Some(&raw),
                Some(&message),
            );
            Err(message)
        }
    }
}

/// A pending video row snapshot taken while holding the DB connection —
/// everything the async poll needs, so no lock crosses an await (坑点2).
struct PendingVideo {
    task_id: String,
    platform_id: String,
    external_id: String,
}

/// Spawns the background loop that drives async video tasks to completion:
/// poll → update progress → on success download the artifact → emit
/// `media-task-update` events for the Studio.
pub fn start_media_poller(app: AppHandle, db: Arc<DbManager>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            // Snapshot pending work, then release the connection before any await.
            let pending: Vec<PendingVideo> = {
                let Ok(conn) = db.get_connection() else { continue };
                let Ok(mut statement) = conn.prepare(
                    "SELECT id, platform_id, external_id FROM media_tasks
                     WHERE kind = 'video' AND status IN ('pending', 'running')
                       AND external_id IS NOT NULL",
                ) else {
                    continue;
                };
                let rows = statement.query_map([], |row| {
                    Ok(PendingVideo {
                        task_id: row.get(0)?,
                        platform_id: row.get(1)?,
                        external_id: row.get(2)?,
                    })
                });
                match rows {
                    Ok(rows) => rows.filter_map(Result::ok).collect(),
                    Err(_) => continue,
                }
            };
            if pending.is_empty() {
                continue;
            }

            for item in pending {
                poll_one_video(&app, &db, &item).await;
            }
        }
    });
}

async fn poll_one_video(app: &AppHandle, db: &Arc<DbManager>, item: &PendingVideo) {
    let Ok((api_key, api_address)) = resolve_media_platform(db, &item.platform_id) else {
        return;
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    else {
        return;
    };
    let url = build_video_poll_url(&api_address, &item.external_id);
    let Ok(response) = client.get(&url).bearer_auth(&api_key).send().await else {
        return; // transient network error — retry next tick
    };
    let Ok(payload) = response.json::<serde_json::Value>().await else {
        return;
    };
    let raw = truncate_raw(&payload);
    let result = parse_video_poll_response(&payload);

    match result.status {
        MediaTaskStatus::Completed => {
            let outcome = match result.video_url.as_deref() {
                Some(video_url) => download_video(&client, video_url, &item.task_id).await,
                None => Err(format!("任务完成但未找到视频 URL（原始响应已记录）")),
            };
            match outcome {
                Ok(path) => {
                    let _ = update_task(
                        db,
                        &item.task_id,
                        MediaTaskStatus::Completed,
                        Some(100),
                        Some(&path),
                        Some(&raw),
                        None,
                    );
                }
                Err(error) => {
                    let _ = update_task(
                        db,
                        &item.task_id,
                        MediaTaskStatus::Failed,
                        None,
                        None,
                        Some(&raw),
                        Some(&error),
                    );
                }
            }
        }
        MediaTaskStatus::Failed => {
            let message = result.error.unwrap_or_else(|| "任务失败".to_string());
            let _ = update_task(
                db,
                &item.task_id,
                MediaTaskStatus::Failed,
                None,
                None,
                Some(&raw),
                Some(&message),
            );
        }
        status => {
            let _ = update_task(db, &item.task_id, status, Some(result.progress), None, None, None);
        }
    }

    // Push the fresh row state to the Studio (best effort).
    if let Ok(task) = get_task(db, &item.task_id) {
        let _ = app.emit("media-task-update", &task);
    }
}

async fn download_video(
    client: &reqwest::Client,
    video_url: &str,
    task_id: &str,
) -> Result<String, String> {
    let bytes = client
        .get(video_url)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|error| format!("下载视频失败: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("读取视频内容失败: {error}"))?;
    let mut path = media_dir()?;
    path.push(format!("{task_id}.mp4"));
    std::fs::write(&path, &bytes).map_err(|error| format!("保存视频失败: {error}"))?;
    Ok(path.to_string_lossy().into_owned())
}
