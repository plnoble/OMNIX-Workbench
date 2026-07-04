//! Media generation protocol layer — image & video (Agnes AI first).
//!
//! Mirrors the `runtime_acp.rs` pattern: this module is the pure wire-format
//! layer (request builders + response parsers, fully unit-tested, no I/O);
//! `commands/media.rs` owns HTTP, files and the task table. Providers are data:
//! adding a new one (Volcano Jimeng, Kling, …) means a new [`MediaProviderKind`]
//! variant, and the compiler forces every dispatch site to handle it.
//!
//! Verified Agnes AI contract (wiki.agnes-ai.com, github.com/AgnesAI-Labs):
//! - Image: `POST {base}/images/generations` `{model, prompt, size}` →
//!   OpenAI-style `data[0].url` or `data[0].b64_json` (both supported here).
//! - Video (async): `POST {base}/videos` → `{video_id, task_id, status}`;
//!   poll `GET {host}/agnesapi?video_id=…` (NOT task_id) → status
//!   `queued|in_progress|completed|failed` + progress. The field carrying the
//!   final video URL is inconsistent in the docs, so the poll parser scans
//!   defensively and the raw response is persisted for diagnosis.

use serde::Serialize;
use serde_json::{json, Value};

/// The wire protocol a media provider speaks. Typed like `AdapterKind` so new
/// providers extend the enum instead of forking the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaProviderKind {
    /// OpenAI-compatible `/images/generations` (Agnes, OpenAI, SiliconFlow…).
    OpenAiImages,
    /// Agnes AI async `/videos` + `/agnesapi?video_id=` polling.
    AgnesVideo,
}

/// Lifecycle of a media task row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl MediaTaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Maps an Agnes/OpenAI-style task status string onto ours.
    pub fn from_provider(status: &str) -> Self {
        match status {
            "queued" | "pending" | "submitted" => Self::Pending,
            "in_progress" | "processing" | "running" => Self::Running,
            "completed" | "succeeded" | "success" => Self::Completed,
            "failed" | "error" | "cancelled" => Self::Failed,
            _ => Self::Running,
        }
    }
}

/// Known media models per provider kind, used as Studio suggestions (a free
/// text field still allows anything).
pub fn suggested_models(kind: MediaProviderKind) -> &'static [&'static str] {
    match kind {
        MediaProviderKind::OpenAiImages => &["agnes-image-2.1-flash", "agnes-image-2.0-flash"],
        MediaProviderKind::AgnesVideo => &["agnes-video-v2.0"],
    }
}

// ───────────────────────── image generation ─────────────────────────

pub fn build_image_request(model: &str, prompt: &str, size: &str) -> Value {
    json!({ "model": model, "prompt": prompt, "size": size })
}

/// A generated image, either hosted (download separately) or inline base64.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageOutput {
    Url(String),
    Base64(String),
}

/// Parses an OpenAI-style images response — `data[0].url` or `data[0].b64_json`
/// (both shapes are in the wild; Agnes documents "URL or Base64 output").
pub fn parse_image_response(value: &Value) -> Result<ImageOutput, String> {
    if let Some(message) = extract_error_message(value) {
        return Err(message);
    }
    let first = value
        .pointer("/data/0")
        .ok_or_else(|| format!("图片响应缺少 data 数组: {value}"))?;
    if let Some(b64) = first.get("b64_json").and_then(Value::as_str) {
        return Ok(ImageOutput::Base64(b64.to_string()));
    }
    if let Some(url) = first.get("url").and_then(Value::as_str) {
        return Ok(ImageOutput::Url(url.to_string()));
    }
    Err(format!("图片响应既无 url 也无 b64_json: {first}"))
}

// ───────────────────────── video generation ─────────────────────────

/// Agnes constraint: `num_frames ≤ 441` and must satisfy the 8n+1 rule.
/// Snaps an arbitrary requested frame count to the nearest valid value.
pub fn normalize_num_frames(requested: u32) -> u32 {
    let clamped = requested.clamp(9, 441);
    // Snap down to the 8n+1 grid (…, 113, 121, 129, …).
    let n = (clamped - 1) / 8;
    8 * n + 1
}

/// Agnes constraint: frame rate 1–60.
pub fn normalize_frame_rate(requested: u32) -> u32 {
    requested.clamp(1, 60)
}

pub struct VideoRequest<'a> {
    pub model: &'a str,
    pub prompt: &'a str,
    /// Source image URL for image-to-video, if any.
    pub image: Option<&'a str>,
    pub width: u32,
    pub height: u32,
    pub num_frames: u32,
    pub frame_rate: u32,
}

pub fn build_video_create_request(request: &VideoRequest) -> Value {
    let mut body = json!({
        "model": request.model,
        "prompt": request.prompt,
        "width": request.width,
        "height": request.height,
        "num_frames": normalize_num_frames(request.num_frames),
        "frame_rate": normalize_frame_rate(request.frame_rate),
    });
    if let Some(image) = request.image {
        body["image"] = Value::String(image.to_string());
    }
    body
}

/// Extracts the id used for polling. Agnes: "Use video_id for video result
/// polling. Do not use task_id" — prefer `video_id`, fall back to the others
/// so a provider that only returns `id` still works.
pub fn parse_video_create_response(value: &Value) -> Result<String, String> {
    if let Some(message) = extract_error_message(value) {
        return Err(message);
    }
    for key in ["video_id", "task_id", "id"] {
        if let Some(id) = value.get(key).and_then(Value::as_str) {
            if !id.trim().is_empty() {
                return Ok(id.to_string());
            }
        }
    }
    Err(format!("视频任务响应缺少 video_id/task_id/id: {value}"))
}

/// Poll endpoint lives at the HOST root (`/agnesapi`), not under `/v1` —
/// derive it from the platform's `/v1` base address.
pub fn build_video_poll_url(api_address: &str, video_id: &str) -> String {
    let base = api_address.trim_end_matches('/');
    let host = base.strip_suffix("/v1").unwrap_or(base);
    format!("{host}/agnesapi?video_id={video_id}")
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoPollResult {
    pub status: MediaTaskStatus,
    pub progress: i64,
    pub video_url: Option<String>,
    pub error: Option<String>,
}

/// Parses a poll response. The docs are inconsistent about which field carries
/// the final URL (one example shows it in `remixed_from_video_id`), so on a
/// completed status we scan the whole payload for the most video-looking URL.
pub fn parse_video_poll_response(value: &Value) -> VideoPollResult {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .map(MediaTaskStatus::from_provider)
        .unwrap_or(MediaTaskStatus::Running);
    let progress = value
        .get("progress")
        .and_then(Value::as_i64)
        .unwrap_or(if status == MediaTaskStatus::Completed { 100 } else { 0 });
    let error = value
        .get("error")
        .filter(|error| !error.is_null())
        .map(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string())
        });
    let video_url = extract_video_url(value);
    VideoPollResult {
        status,
        progress,
        video_url,
        error,
    }
}

/// Collects every string in the payload that looks like an http(s) URL and
/// picks the most plausible video link (prefers .mp4 / video-ish paths).
pub fn extract_video_url(value: &Value) -> Option<String> {
    let mut candidates = Vec::new();
    collect_urls(value, &mut candidates);
    if candidates.is_empty() {
        return None;
    }
    candidates
        .iter()
        .find(|url| url.contains(".mp4") || url.contains(".webm") || url.contains(".mov"))
        .or_else(|| candidates.iter().find(|url| url.to_lowercase().contains("video")))
        .or_else(|| candidates.first())
        .cloned()
}

fn collect_urls(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if text.starts_with("http://") || text.starts_with("https://") {
                out.push(text.clone());
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_urls(item, out)),
        Value::Object(map) => map.values().for_each(|item| collect_urls(item, out)),
        _ => {}
    }
}

/// Pulls a human-readable message out of an OpenAI-style error payload.
fn extract_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    if error.is_null() {
        return None;
    }
    Some(
        error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_request_carries_model_prompt_size() {
        let body = build_image_request("agnes-image-2.1-flash", "一只在海边散步的猫", "1024x768");
        assert_eq!(body["model"], "agnes-image-2.1-flash");
        assert_eq!(body["prompt"], "一只在海边散步的猫");
        assert_eq!(body["size"], "1024x768");
    }

    #[test]
    fn image_response_parses_url_and_b64() {
        let url = json!({ "data": [{ "url": "https://cdn.example.com/a.png" }] });
        assert_eq!(
            parse_image_response(&url).unwrap(),
            ImageOutput::Url("https://cdn.example.com/a.png".into())
        );
        let b64 = json!({ "data": [{ "b64_json": "aGVsbG8=" }] });
        assert_eq!(
            parse_image_response(&b64).unwrap(),
            ImageOutput::Base64("aGVsbG8=".into())
        );
        let error = json!({ "error": { "message": "quota exceeded" } });
        assert_eq!(parse_image_response(&error).unwrap_err(), "quota exceeded");
        let empty = json!({ "data": [] });
        assert!(parse_image_response(&empty).is_err());
    }

    #[test]
    fn num_frames_snap_to_8n_plus_1_within_limits() {
        assert_eq!(normalize_num_frames(121), 121); // already valid
        assert_eq!(normalize_num_frames(120), 113); // snap down to grid
        assert_eq!(normalize_num_frames(1), 9); // floor
        assert_eq!(normalize_num_frames(9999), 441); // ceil (441 = 8*55+1)
        assert_eq!(normalize_frame_rate(0), 1);
        assert_eq!(normalize_frame_rate(120), 60);
    }

    #[test]
    fn video_create_request_includes_optional_image() {
        let request = build_video_create_request(&VideoRequest {
            model: "agnes-video-v2.0",
            prompt: "海边日落",
            image: Some("https://img.example.com/src.png"),
            width: 1152,
            height: 768,
            num_frames: 121,
            frame_rate: 24,
        });
        assert_eq!(request["image"], "https://img.example.com/src.png");
        assert_eq!(request["num_frames"], 121);
        let no_image = build_video_create_request(&VideoRequest {
            model: "agnes-video-v2.0",
            prompt: "x",
            image: None,
            width: 1152,
            height: 768,
            num_frames: 121,
            frame_rate: 24,
        });
        assert!(no_image.get("image").is_none());
    }

    #[test]
    fn video_create_response_prefers_video_id() {
        let full = json!({ "id": "task_1", "task_id": "task_1", "video_id": "video_9" });
        assert_eq!(parse_video_create_response(&full).unwrap(), "video_9");
        let id_only = json!({ "id": "task_1" });
        assert_eq!(parse_video_create_response(&id_only).unwrap(), "task_1");
        let error = json!({ "error": { "message": "bad prompt" } });
        assert!(parse_video_create_response(&error).is_err());
    }

    #[test]
    fn poll_url_derives_from_v1_base() {
        assert_eq!(
            build_video_poll_url("https://apihub.agnes-ai.com/v1", "video_9"),
            "https://apihub.agnes-ai.com/agnesapi?video_id=video_9"
        );
        assert_eq!(
            build_video_poll_url("https://apihub.agnes-ai.com/v1/", "video_9"),
            "https://apihub.agnes-ai.com/agnesapi?video_id=video_9"
        );
    }

    #[test]
    fn poll_response_maps_states_and_finds_url_defensively() {
        let running = json!({ "status": "in_progress", "progress": 42 });
        let parsed = parse_video_poll_response(&running);
        assert_eq!(parsed.status, MediaTaskStatus::Running);
        assert_eq!(parsed.progress, 42);
        assert!(parsed.video_url.is_none());

        // The documented-but-odd shape: the URL hides in remixed_from_video_id.
        let done = json!({
            "status": "completed",
            "progress": 100,
            "remixed_from_video_id": "https://storage.googleapis.com/x/out.mp4",
            "error": null
        });
        let parsed = parse_video_poll_response(&done);
        assert_eq!(parsed.status, MediaTaskStatus::Completed);
        assert_eq!(
            parsed.video_url.as_deref(),
            Some("https://storage.googleapis.com/x/out.mp4")
        );

        // Nested + multiple URLs: prefer the video-looking one.
        let nested = json!({
            "status": "completed",
            "data": { "thumbnail": "https://cdn.example.com/t.jpg", "video_url": "https://cdn.example.com/v.mp4" }
        });
        assert_eq!(
            parse_video_poll_response(&nested).video_url.as_deref(),
            Some("https://cdn.example.com/v.mp4")
        );

        let failed = json!({ "status": "failed", "error": { "message": "nsfw rejected" } });
        let parsed = parse_video_poll_response(&failed);
        assert_eq!(parsed.status, MediaTaskStatus::Failed);
        assert_eq!(parsed.error.as_deref(), Some("nsfw rejected"));
    }

    #[test]
    fn status_mapping_covers_provider_vocabulary() {
        assert_eq!(MediaTaskStatus::from_provider("queued"), MediaTaskStatus::Pending);
        assert_eq!(MediaTaskStatus::from_provider("in_progress"), MediaTaskStatus::Running);
        assert_eq!(MediaTaskStatus::from_provider("completed"), MediaTaskStatus::Completed);
        assert_eq!(MediaTaskStatus::from_provider("failed"), MediaTaskStatus::Failed);
        assert_eq!(MediaTaskStatus::from_provider("weird"), MediaTaskStatus::Running);
    }
}
