//! OfficeCLI foundation — the muscle behind OMNIX's office features.
//!
//! OfficeCLI (`iOfficeAI/OfficeCLI`, Apache-2.0, single binary) does the file
//! work OMNIX deliberately does not hand-write: pptx schema validation and
//! issue scans (质检门), pptx text extraction (导入), and later docx/xlsx.
//!
//! Management policy — unlike agent CLIs (where a PATH copy is authoritative),
//! OMNIX prefers its **own pinned copy**: QA verdicts and import parsers depend
//! on this exact version's output shapes, and the user's PATH copy can be
//! months stale (an April build was found in the wild). Resolution order is
//! managed → PATH-fallback, the reverse of `agent.rs`.
//!
//! Known side effect, accepted and managed: officecli auto-refreshes the skill
//! files it previously installed whenever its version changes (no opt-out flag
//! as of 1.0.136). We do not fight it — the skill auto-update engine in
//! `commands/skill_updates.rs` detects the refreshed source and folds it back
//! into the central store as a reviewed, backed-up update.

use std::path::PathBuf;
use std::process::Stdio;

use serde::Serialize;

use crate::proc::NoWindow;

/// Pinned release. Bump deliberately: QA/import parsing is validated against
/// this version's output shapes (see `verify` notes in the repo memory).
pub const OFFICECLI_VERSION: &str = "1.0.136";

fn release_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("officecli-win-x64.exe")
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        Some("officecli-win-arm64.exe")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("officecli-mac-arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("officecli-mac-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("officecli-linux-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("officecli-linux-arm64")
    } else {
        None
    }
}

/// `~/.omnix/tools/officecli/` — fixed under the omnix root (like the DB), not
/// a configurable storage key: QA must keep working even if the user relocates
/// backups/exports.
pub fn managed_dir() -> PathBuf {
    crate::storage::omnix_root().join("tools").join("officecli")
}

fn managed_exe() -> PathBuf {
    managed_dir().join(if cfg!(windows) { "officecli.exe" } else { "officecli" })
}

/// Resolved binary + where it came from ("managed" | "system").
pub fn resolve() -> Option<(String, &'static str)> {
    let managed = managed_exe();
    if managed.is_file() {
        return Some((managed.to_string_lossy().to_string(), "managed"));
    }
    which::which("officecli")
        .ok()
        .map(|p| (p.to_string_lossy().to_string(), "system"))
}

/// Run officecli with a hard timeout. Every invocation goes through here:
/// `.no_window()` (Windows console-flood rule) and stdin closed.
pub async fn run(args: &[&str], timeout_secs: u64) -> Result<std::process::Output, String> {
    let (exe, _) = resolve().ok_or(OFFICECLI_MISSING)?;
    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(args)
        .no_window()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let fut = cmd.output();
    match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), fut).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(format!("OfficeCLI 启动失败: {e}")),
        Err(_) => Err(format!("OfficeCLI 超时（>{timeout_secs}s）: {}", args.join(" "))),
    }
}

pub const OFFICECLI_MISSING: &str =
    "OfficeCLI 未安装——到 技能中心/演示 提示处一键安装，或手动安装后重试";

/// Download the pinned release into the managed dir, verifying against the
/// release's SHA256SUMS before the binary is moved into place.
pub async fn install_managed() -> Result<String, String> {
    let asset = release_asset().ok_or("当前平台没有对应的 OfficeCLI 发行包")?;
    let base =
        format!("https://github.com/iOfficeAI/OfficeCLI/releases/download/v{OFFICECLI_VERSION}");
    let client = reqwest::Client::builder()
        .user_agent("omnix-workbench")
        .build()
        .map_err(|e| e.to_string())?;

    let sums = client
        .get(format!("{base}/SHA256SUMS"))
        .send()
        .await
        .map_err(|e| format!("下载校验清单失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("校验清单不可用: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let expected = sums
        .lines()
        .find(|l| l.contains(asset))
        .and_then(|l| l.split_whitespace().next())
        .ok_or_else(|| format!("SHA256SUMS 里找不到 {asset}"))?
        .to_lowercase();

    let bytes = client
        .get(format!("{base}/{asset}"))
        .send()
        .await
        .map_err(|e| format!("下载 OfficeCLI 失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 OfficeCLI 失败: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    use sha2::{Digest, Sha256};
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected {
        return Err(format!(
            "OfficeCLI 校验失败：expected {expected}, got {actual} —— 已放弃安装"
        ));
    }

    let dir = managed_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    // Atomic-ish: write next to the target, then rename over it.
    let tmp = dir.join("officecli.download");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("写入失败: {e}"))?;
    let target = managed_exe();
    std::fs::rename(&tmp, &target).map_err(|e| format!("落位失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }
    Ok(target.to_string_lossy().to_string())
}

/// Probe `--version`. officecli may prefix a "refreshed N skill file(s)" notice
/// line, so take the last line that looks like a version.
pub async fn probe_version() -> Option<String> {
    let output = run(&["--version"], 30).await.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && l.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

// ── pptx 质检门 ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PptxQa {
    /// None = officecli unavailable, QA skipped (export still succeeds).
    pub ran: bool,
    pub schema_ok: bool,
    pub issue_count: usize,
    /// Human-readable lines: validate verdict + each issue.
    pub detail: Vec<String>,
}

/// Validate an exported pptx: OpenXML schema + content issue scan. Never fails
/// the export — a missing officecli or a QA error degrades to `ran: false`.
pub async fn pptx_qa(path: &str) -> PptxQa {
    let mut detail = Vec::new();

    let validate = match run(&["validate", path], 120).await {
        Ok(o) => o,
        Err(e) => {
            return PptxQa { ran: false, schema_ok: false, issue_count: 0, detail: vec![e] };
        }
    };
    let vtext = format!(
        "{}{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    let schema_ok = validate.status.success()
        && (vtext.contains("no errors") || vtext.to_lowercase().contains("passed"));
    detail.push(if schema_ok {
        "OpenXML schema 校验通过".to_string()
    } else {
        format!("schema 校验未通过：{}", vtext.trim().chars().take(400).collect::<String>())
    });

    let mut issue_count = 0;
    if let Ok(issues) = run(&["view", path, "issues"], 120).await {
        let itext = String::from_utf8_lossy(&issues.stdout).to_string();
        // Shape (1.0.136): "Found N issue(s):" then one line per issue.
        for line in itext.lines().map(str::trim) {
            if line.is_empty() || line.starts_with("Found ") {
                continue;
            }
            issue_count += 1;
            if detail.len() < 12 {
                detail.push(format!("问题：{line}"));
            }
        }
        if issue_count == 0 {
            detail.push("内容扫描 0 问题".to_string());
        }
    }

    PptxQa { ran: true, schema_ok, issue_count, detail }
}

// ── pptx 导入 ────────────────────────────────────────────────────────────────

/// One slide's extracted text. `lines` flattens the slide's shapes: the first
/// entry of `texts` is usually the title shape; a body shape carries its
/// bullets joined by `\n` inside a single entry.
pub struct ExtractedSlide {
    pub lines: Vec<String>,
    pub notes: String,
}

/// `view <file> text --json` shape (verified against officecli 1.0.136):
/// `{"success":true,"data":{"totalSlides":N,"slides":[{"index":1,"path":"/slide[1]","texts":["…"]}]}}`
fn parse_text_json(raw: &str) -> Result<Vec<ExtractedSlide>, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("OfficeCLI 输出不是 JSON: {e}"))?;
    let slides = value
        .pointer("/data/slides")
        .and_then(|s| s.as_array())
        .ok_or("OfficeCLI 输出缺少 data.slides")?;
    let extracted: Vec<ExtractedSlide> = slides
        .iter()
        .map(|slide| {
            let lines = slide
                .pointer("/texts")
                .and_then(|t| t.as_array())
                .map(|texts| {
                    texts
                        .iter()
                        .filter_map(|t| t.as_str())
                        .flat_map(|t| t.split('\n'))
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            ExtractedSlide { lines, notes: String::new() }
        })
        .collect();
    if extracted.is_empty() {
        return Err("没有从该 pptx 里提取到任何页面".to_string());
    }
    Ok(extracted)
}

/// `query <file> notes --json` shape (verified against officecli 1.0.136):
/// `{"success":true,"data":{"results":[{"path":"/slide[3]/notes","text":"…"}]}}`
fn apply_notes_json(raw: &str, slides: &mut [ExtractedSlide]) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else { return };
    let Some(results) = value.pointer("/data/results").and_then(|r| r.as_array()) else {
        return;
    };
    for result in results {
        let Some(path) = result.pointer("/path").and_then(|p| p.as_str()) else { continue };
        let Some(idx) = path
            .strip_prefix("/slide[")
            .and_then(|rest| rest.split(']').next())
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(text) = result.pointer("/text").and_then(|t| t.as_str()) else { continue };
        if let Some(slide) = slides.get_mut(idx.saturating_sub(1)) {
            slide.notes = text.trim().to_string();
        }
    }
}

/// Extract per-slide text + speaker notes from any pptx on disk.
pub async fn extract_pptx_text(path: &str) -> Result<Vec<ExtractedSlide>, String> {
    let output = run(&["view", path, "text", "--json"], 180).await?;
    if !output.status.success() {
        return Err(format!(
            "OfficeCLI 读取失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut slides = parse_text_json(&String::from_utf8_lossy(&output.stdout))?;
    if let Ok(notes_out) = run(&["query", path, "notes", "--json"], 120).await {
        apply_notes_json(&String::from_utf8_lossy(&notes_out.stdout), &mut slides);
    }
    Ok(slides)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim `view text --json` fragment from officecli 1.0.136.
    const TEXT_JSON: &str = r#"{"success":true,"data":{"totalSlides":2,"slides":[
        {"index":1,"path":"/slide[1]","texts":["封面：全版式验证","副标题 & 特殊字符 <>","OMNIX · 内部样例"]},
        {"index":2,"path":"/slide[2]","texts":["要点页","普通要点\n带 加粗 的要点","OMNIX · 内部样例"]}]}}"#;

    /// Verbatim `query notes --json` fragment from officecli 1.0.136.
    const NOTES_JSON: &str = r#"{"success":true,"data":{"matches":1,"results":[
        {"path":"/slide[2]/notes","type":"notes","text":"演讲备注：先讲背景。","childCount":0}]}}"#;

    #[test]
    fn parses_real_view_text_json() {
        let slides = parse_text_json(TEXT_JSON).unwrap();
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].lines[0], "封面：全版式验证");
        // A body shape's newline-joined bullets are flattened into lines.
        assert_eq!(slides[1].lines, vec!["要点页", "普通要点", "带 加粗 的要点", "OMNIX · 内部样例"]);
    }

    #[test]
    fn notes_land_on_the_right_slide() {
        let mut slides = parse_text_json(TEXT_JSON).unwrap();
        apply_notes_json(NOTES_JSON, &mut slides);
        assert_eq!(slides[0].notes, "");
        assert_eq!(slides[1].notes, "演讲备注：先讲背景。");
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(parse_text_json("not json").is_err());
        assert!(parse_text_json(r#"{"success":true,"data":{}}"#).is_err());
        let mut slides = parse_text_json(TEXT_JSON).unwrap();
        apply_notes_json("garbage", &mut slides); // must be a no-op
        assert_eq!(slides[1].lines.len(), 4);
    }
}
