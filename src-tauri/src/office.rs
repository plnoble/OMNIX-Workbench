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

// ── Word: Markdown ⇄ docx ────────────────────────────────────────────────────

/// One officecli `batch` item. The batch schema (help text, 1.0.136): each item
/// is an object whose `command` is the bare verb; verb args are sibling fields
/// (`parent`, `path`, `type`, `props`).
fn batch_add(parent: &str, ty: &str, props: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "command": "add", "parent": parent, "type": ty, "props": props })
}

/// Markdown → officecli batch commands building a styled docx.
///
/// Supported constructs (the Write space's working set): `#`–`###` headings,
/// `-`/`*` bullets, `N.` ordered items, `>` quotes, `|`-tables, plain
/// paragraphs, and inline `**bold**` (leading segment only — officecli's
/// implicit run carries one format; full inline runs are a later refinement).
/// Brand: heading color = `brand.primary`, body font = `brand.font`.
pub fn markdown_to_docx_batch(md: &str, brand: Option<&crate::slides::Brand>) -> Vec<serde_json::Value> {
    let heading_color = brand
        .map(|b| b.primary.trim())
        .filter(|c| c.starts_with('#') && c.len() == 7)
        .unwrap_or("")
        .to_string();
    let font = brand.map(|b| b.font.trim()).unwrap_or("").to_string();

    let decorate = |props: &mut serde_json::Map<String, serde_json::Value>, is_heading: bool| {
        if !font.is_empty() {
            props.insert("font".into(), font.clone().into());
        }
        if is_heading && !heading_color.is_empty() {
            props.insert("color".into(), heading_color.clone().into());
        }
    };
    // Inline `**bold**`: keep the text clean; bold the whole paragraph only when
    // it is entirely wrapped (common for emphasis lines).
    let clean = |s: &str| s.replace("**", "");
    let fully_bold =
        |s: &str| s.starts_with("**") && s.ends_with("**") && s.matches("**").count() == 2;

    let mut out = Vec::new();
    let mut lines = md.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        // Markdown table block: header | separator | rows.
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let parse_row = |l: &str| -> Vec<String> {
                l.trim().trim_matches('|').split('|').map(|c| clean(c.trim())).collect()
            };
            let header = parse_row(trimmed);
            let mut rows: Vec<Vec<String>> = Vec::new();
            while let Some(next) = lines.peek() {
                let t = next.trim();
                if !(t.starts_with('|') && t.ends_with('|')) {
                    break;
                }
                let cells = parse_row(t);
                lines.next();
                // Skip the |---|---| separator row.
                if cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')) {
                    continue;
                }
                rows.push(cells);
            }
            let cols = header.len().max(1);
            out.push(batch_add(
                "/body",
                "table",
                serde_json::json!({ "rows": rows.len() + 1, "cols": cols }),
            ));
            // Fill cells via `set` on the table just appended (last table index
            // is unknown mid-batch, so address the row/cell under /body/tbl[-1]
            // is unsupported — instead set by explicit text on the freshly added
            // table path officecli reports positionally: tables are appended in
            // order, so the Nth table batch-added is /body/table[N]. We track it.
            let table_index = out.iter().filter(|c| c["type"] == "table").count();
            for (ci, cell) in header.iter().enumerate().take(cols) {
                out.push(serde_json::json!({
                    "command": "set",
                    "path": format!("/body/table[{table_index}]/tr[1]/tc[{}]", ci + 1),
                    "props": { "text": cell, "bold": true }
                }));
            }
            for (ri, row) in rows.iter().enumerate() {
                for (ci, cell) in row.iter().enumerate().take(cols) {
                    out.push(serde_json::json!({
                        "command": "set",
                        "path": format!("/body/table[{table_index}]/tr[{}]/tc[{}]", ri + 2, ci + 1),
                        "props": { "text": cell }
                    }));
                }
            }
            continue;
        }

        let mut props = serde_json::Map::new();
        let is_heading;
        if let Some(rest) = trimmed.strip_prefix("### ") {
            props.insert("text".into(), clean(rest).into());
            props.insert("style".into(), "Heading3".into());
            props.insert("size".into(), "14pt".into());
            props.insert("bold".into(), true.into());
            is_heading = true;
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            props.insert("text".into(), clean(rest).into());
            props.insert("style".into(), "Heading2".into());
            props.insert("size".into(), "16pt".into());
            props.insert("bold".into(), true.into());
            is_heading = true;
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            props.insert("text".into(), clean(rest).into());
            props.insert("style".into(), "Heading1".into());
            props.insert("size".into(), "20pt".into());
            props.insert("bold".into(), true.into());
            is_heading = true;
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            props.insert("text".into(), clean(rest).into());
            props.insert("listStyle".into(), "bullet".into());
            is_heading = false;
        } else if trimmed.len() > 2
            && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            && (trimmed[1..].starts_with(". ") || trimmed[1..].starts_with("、"))
        {
            let rest = trimmed.splitn(2, [' ', '、']).nth(1).unwrap_or("");
            props.insert("text".into(), clean(rest).into());
            props.insert("listStyle".into(), "ordered".into());
            is_heading = false;
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            props.insert("text".into(), clean(rest).into());
            props.insert("style".into(), "Quote".into());
            props.insert("italic".into(), true.into());
            is_heading = false;
        } else {
            props.insert("text".into(), clean(trimmed).into());
            if fully_bold(trimmed) {
                props.insert("bold".into(), true.into());
            }
            is_heading = false;
        }
        decorate(&mut props, is_heading);
        out.push(batch_add("/body", "paragraph", serde_json::Value::Object(props)));
    }
    out
}

/// Build a docx at `out_path` from markdown: create + one batch. Returns QA-ish
/// detail lines (batch summary) for the UI toast.
pub async fn markdown_to_docx(
    md: &str,
    out_path: &str,
    brand: Option<&crate::slides::Brand>,
    footer: Option<&str>,
) -> Result<(), String> {
    let mut commands = markdown_to_docx_batch(md, brand);
    if let Some(footer) = footer.filter(|f| !f.trim().is_empty()) {
        commands.push(batch_add(
            "/body",
            "paragraph",
            serde_json::json!({ "text": footer, "size": "9pt", "color": "#888888", "align": "center", "spaceBefore": "18pt" }),
        ));
    }
    if commands.is_empty() {
        return Err("没有可导出的内容".to_string());
    }
    let create = run(&["create", out_path, "--force"], 60).await?;
    if !create.status.success() {
        // --force may not exist on `create`; retry plain after removing the file.
        let _ = std::fs::remove_file(out_path);
        let retry = run(&["create", out_path], 60).await?;
        if !retry.status.success() {
            return Err(format!(
                "创建 docx 失败: {}",
                String::from_utf8_lossy(&retry.stderr).trim()
            ));
        }
    }
    let json = serde_json::to_string(&commands).map_err(|e| e.to_string())?;
    let output = run(&["batch", out_path, "--commands", &json], 300).await?;
    if !output.status.success() {
        return Err(format!(
            "写入 docx 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim().chars().take(500).collect::<String>()
        ));
    }
    Ok(())
}

/// docx → Markdown via the full body tree (`get /body --json`), preserving
/// heading levels, bullet/ordered lists, bold paragraphs, and tables.
pub async fn docx_to_markdown(path: &str) -> Result<String, String> {
    let output = run(&["get", path, "/body", "--json"], 180).await?;
    if !output.status.success() {
        return Err(format!(
            "OfficeCLI 读取失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
            .map_err(|e| format!("OfficeCLI 输出不是 JSON: {e}"))?;
    let results = value
        .pointer("/data/results")
        .and_then(|r| r.as_array())
        .ok_or("OfficeCLI 输出缺少 data.results")?;

    let mut md = String::new();
    for root in results {
        walk_docx_node(root, &mut md);
    }
    if md.trim().is_empty() {
        return Err("没有从该 docx 里提取到内容".to_string());
    }
    Ok(md)
}

fn walk_docx_node(node: &serde_json::Value, md: &mut String) {
    let ty = node.pointer("/type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "paragraph" => {
            let text = node.pointer("/text").and_then(|t| t.as_str()).unwrap_or("").trim();
            if text.is_empty() {
                return;
            }
            let format = node.pointer("/format").cloned().unwrap_or_default();
            let style = format.pointer("/style").and_then(|s| s.as_str()).unwrap_or("");
            let list = format
                .pointer("/listStyle")
                .or_else(|| format.pointer("/list"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let bold = format.pointer("/bold").and_then(|b| b.as_bool()).unwrap_or(false);
            let line = match style {
                "Heading1" | "heading 1" => format!("# {text}"),
                "Heading2" | "heading 2" => format!("## {text}"),
                "Heading3" | "Heading4" | "heading 3" => format!("### {text}"),
                "Quote" | "IntenseQuote" => format!("> {text}"),
                _ if list == "bullet" => format!("- {text}"),
                _ if list == "ordered" => format!("1. {text}"),
                _ if bold => format!("**{text}**"),
                _ => text.to_string(),
            };
            md.push_str(&line);
            md.push_str("\n\n");
        }
        "table" => {
            let rows: Vec<Vec<String>> = node
                .pointer("/children")
                .and_then(|c| c.as_array())
                .map(|trs| {
                    trs.iter()
                        .filter(|tr| tr.pointer("/type").and_then(|t| t.as_str()) == Some("tableRow"))
                        .map(|tr| {
                            tr.pointer("/children")
                                .and_then(|c| c.as_array())
                                .map(|tcs| {
                                    tcs.iter()
                                        .map(|tc| {
                                            tc.pointer("/text")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("")
                                                .trim()
                                                .replace('|', "\\|")
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some((head, body)) = rows.split_first() {
                md.push_str(&format!("| {} |\n", head.join(" | ")));
                md.push_str(&format!("|{}\n", " --- |".repeat(head.len())));
                for row in body {
                    md.push_str(&format!("| {} |\n", row.join(" | ")));
                }
                md.push('\n');
            }
        }
        _ => {
            if let Some(children) = node.pointer("/children").and_then(|c| c.as_array()) {
                for child in children {
                    walk_docx_node(child, md);
                }
            }
        }
    }
}

// ── merge 批量生成 + 通用预览 + Excel ───────────────────────────────────────

/// `merge <template> <output> --data <json>` for one record.
pub async fn merge_one(template: &str, out_path: &str, data_json: &str) -> Result<(), String> {
    let output = run(&["merge", template, out_path, "--data", data_json, "--force"], 120).await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().chars().take(300).collect());
    }
    Ok(())
}

/// Any office file → self-contained HTML (docx/xlsx/pptx all support `view html`).
pub async fn preview_html(path: &str) -> Result<String, String> {
    let output = run(&["view", path, "html"], 180).await?;
    if !output.status.success() {
        return Err(format!(
            "预览失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// xlsx text snapshot (cells as `A1=value` lines) — the model-facing view.
pub async fn xlsx_text(path: &str) -> Result<String, String> {
    let output = run(&["view", path, "text"], 120).await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a validated officecli batch against a workbook/document.
pub async fn apply_batch(path: &str, commands_json: &str) -> Result<String, String> {
    // Parse first so a malformed model reply never reaches the CLI.
    let parsed: serde_json::Value = serde_json::from_str(commands_json)
        .map_err(|e| format!("批量命令不是合法 JSON: {e}"))?;
    if !parsed.is_array() || parsed.as_array().is_some_and(|a| a.is_empty()) {
        return Err("批量命令必须是非空 JSON 数组".to_string());
    }
    let output = run(&["batch", path, "--commands", commands_json], 300).await?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
        return Err(format!(
            "执行失败: {}",
            String::from_utf8_lossy(&output.stderr).trim().chars().take(500).collect::<String>()
        ));
    }
    Ok(stdout)
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
    fn markdown_maps_to_docx_batch() {
        let md = "# 标题\n\n正文段落。\n\n- 要点甲\n- **要点乙**\n\n1. 第一步\n\n> 引用一句\n\n| 列A | 列B |\n| --- | --- |\n| 1 | 2 |\n";
        let brand = crate::slides::Brand { primary: "#3b82f6".into(), font: "Inter".into(), ..Default::default() };
        let batch = markdown_to_docx_batch(md, Some(&brand));
        let json = serde_json::to_string(&batch).unwrap();
        assert!(json.contains(r#""style":"Heading1""#), "heading style");
        assert!(json.contains(r##""color":"#3b82f6""##), "brand color on headings");
        assert!(json.contains(r#""listStyle":"bullet""#), "bullets");
        assert!(json.contains(r#""listStyle":"ordered""#), "ordered list");
        assert!(json.contains(r#""style":"Quote""#), "quote");
        assert!(json.contains("要点乙") && !json.contains("**"), "bold markers stripped");
        // Table: one add + 4 cell sets (2 header + 2 body).
        assert!(json.contains(r#""rows":2"#) && json.contains("/body/table[1]/tr[2]/tc[2]"));
        // Separator row |---|---| never becomes a content row.
        assert!(!json.contains("---"));
    }

    #[test]
    fn docx_tree_walks_back_to_markdown() {
        let tree = serde_json::json!({"data":{"results":[{"type":"body","children":[
            {"type":"paragraph","text":"大标题","format":{"style":"Heading1"}},
            {"type":"paragraph","text":"正文","format":{}},
            {"type":"paragraph","text":"要点","format":{"listStyle":"bullet"}},
            {"type":"table","children":[
                {"type":"tableRow","children":[{"type":"tableCell","text":"甲"},{"type":"tableCell","text":"乙"}]},
                {"type":"tableRow","children":[{"type":"tableCell","text":"1"},{"type":"tableCell","text":"2"}]}
            ]}
        ]}]}});
        let mut md = String::new();
        for r in tree.pointer("/data/results").unwrap().as_array().unwrap() {
            walk_docx_node(r, &mut md);
        }
        assert!(md.contains("# 大标题"));
        assert!(md.contains("- 要点"));
        assert!(md.contains("| 甲 | 乙 |"));
        assert!(md.contains("| 1 | 2 |"));
    }

    /// Dumps the md→docx batch JSON to temp for a real officecli round-trip
    /// (mirrors the pptx dump convention: `cargo test --lib -- --ignored`).
    #[test]
    #[ignore]
    fn dump_docx_batch_for_external_validation() {
        let md = "# OMNIX 长文导出验证\n\n首段正文，包含中文与 English。\n\n## 第二章 要点\n\n- 要点甲\n- **要点乙**\n\n1. 第一步\n2. 第二步\n\n> 引用：精而强。\n\n| 指标 | 数值 |\n| --- | --- |\n| 覆盖率 | 100% |\n| 警告 | 0 |\n";
        let brand = crate::slides::Brand {
            primary: "#3b82f6".into(),
            font: "Microsoft YaHei".into(),
            footer: "OMNIX · 导出验证".into(),
            ..Default::default()
        };
        let batch = markdown_to_docx_batch(md, Some(&brand));
        let path = std::env::temp_dir().join("omnix_docx_batch.json");
        std::fs::write(&path, serde_json::to_string_pretty(&batch).unwrap()).unwrap();
        println!("batch written to: {}", path.display());
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
