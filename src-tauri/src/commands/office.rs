//! Office 底座命令：OfficeCLI 状态/托管安装 + pptx 导入。
//! 质检门直接挂在 `slides::export_deck_pptx` 里（导出即质检）。

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::DbManager;
use crate::office;
use crate::slides::{Deck, Slide};

#[derive(Debug, Clone, Serialize)]
pub struct OfficeStatus {
    pub installed: bool,
    pub path: Option<String>,
    /// "managed" | "system"
    pub kind: Option<String>,
    pub version: Option<String>,
    pub pinned_version: String,
    /// The managed copy is either missing or not the pinned version.
    pub update_available: bool,
    /// officecli skill's state in the skill pool, if collected.
    pub skill_pool: Option<String>,
    pub skill_reviewed: bool,
}

#[tauri::command]
pub async fn office_status(db: State<'_, Arc<DbManager>>) -> Result<OfficeStatus, String> {
    let resolved = office::resolve();
    let (path, kind) = match &resolved {
        Some((p, k)) => (Some(p.clone()), Some((*k).to_string())),
        None => (None, None),
    };
    let version = if resolved.is_some() { office::probe_version().await } else { None };

    let (skill_pool, skill_reviewed) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT pool, reviewed_at IS NOT NULL FROM skills WHERE name = 'officecli'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
        )
        .map(|(p, r)| (Some(p), r))
        .unwrap_or((None, false))
    };

    Ok(OfficeStatus {
        installed: resolved.is_some(),
        update_available: kind.as_deref() != Some("managed")
            || version.as_deref() != Some(office::OFFICECLI_VERSION),
        path,
        kind,
        version,
        pinned_version: office::OFFICECLI_VERSION.to_string(),
        skill_pool,
        skill_reviewed,
    })
}

/// Download + verify + place the pinned OfficeCLI into the managed dir.
#[tauri::command]
pub async fn office_install() -> Result<String, String> {
    office::install_managed().await
}

// ── P1 Word ─────────────────────────────────────────────────────────────────

/// 写作空间 Markdown → 带样式 docx（可选品牌母版：标题色/正文字体/页脚）。
#[tauri::command]
pub async fn export_write_docx(
    markdown: String,
    title: String,
    brand_name: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    if markdown.trim().is_empty() {
        return Err("没有可导出的内容".to_string());
    }
    let brand: Option<crate::slides::Brand> = match brand_name.filter(|b| !b.is_empty()) {
        Some(name) => {
            let conn = db.get_connection().map_err(|e| e.to_string())?;
            conn.query_row(
                "SELECT model_json FROM deck_brands WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
        }
        None => None,
    };
    let footer = brand.as_ref().map(|b| b.footer.clone()).filter(|f| !f.trim().is_empty());
    let safe = super::slides::sanitize_filename(if title.trim().is_empty() { "未命名文档" } else { title.trim() });
    let out = crate::storage::exports_dir().join(format!("{safe}.docx"));
    let out_str = out.to_string_lossy().to_string();
    office::markdown_to_docx(&markdown, &out_str, brand.as_ref(), footer.as_deref()).await?;
    super::slides::reveal_in_folder(&out);
    Ok(out_str)
}

/// 现有 docx → Markdown（进写作空间继续编辑）。
#[tauri::command]
pub async fn import_docx_markdown(file_path: String) -> Result<String, String> {
    if !file_path.to_lowercase().ends_with(".docx") {
        return Err("只支持 .docx 文件".to_string());
    }
    office::docx_to_markdown(&file_path).await
}

/// merge 批量生成：模板 + JSON 数组 → 每条记录一份产物。
/// `name_key` 指定用哪个字段命名产物（缺省用序号）。
#[tauri::command]
pub async fn office_merge_batch(
    template: String,
    data_json: String,
    name_key: Option<String>,
) -> Result<Vec<String>, String> {
    let ext = std::path::Path::new(&template)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !matches!(ext.as_str(), "docx" | "xlsx" | "pptx") {
        return Err("模板必须是 .docx / .xlsx / .pptx".to_string());
    }
    let value: serde_json::Value =
        serde_json::from_str(&data_json).map_err(|e| format!("数据不是合法 JSON: {e}"))?;
    let records: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(items) if !items.is_empty() => items,
        serde_json::Value::Object(_) => vec![value],
        _ => return Err("数据要是 JSON 对象或非空数组".to_string()),
    };
    let stem = std::path::Path::new(&template)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "merged".into());
    let out_dir = crate::storage::exports_dir().join(format!(
        "{}_批量_{}",
        super::slides::sanitize_filename(&stem),
        chrono::Local::now().format("%m%d_%H%M%S")
    ));
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let mut outputs = Vec::new();
    for (i, record) in records.iter().enumerate() {
        let label = name_key
            .as_deref()
            .and_then(|k| record.pointer(&format!("/{k}")))
            .and_then(|v| v.as_str())
            .map(|s| super::slides::sanitize_filename(s))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{:03}", i + 1));
        let out = out_dir.join(format!("{label}.{ext}"));
        office::merge_one(&template, &out.to_string_lossy(), &record.to_string())
            .await
            .map_err(|e| format!("第 {} 条失败: {e}", i + 1))?;
        outputs.push(out.to_string_lossy().to_string());
    }
    super::slides::reveal_in_folder(&out_dir);
    Ok(outputs)
}

/// AI 长文两阶段之一：出章节大纲（标题+一句概要）。
#[tauri::command]
pub async fn write_outline_ai(
    topic: String,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<WriteSection>, String> {
    if topic.trim().is_empty() {
        return Err("先说明要写什么".to_string());
    }
    let prompt = format!(
        "你是专业写作者。只输出 JSON 数组，不要解释、不要代码围栏。为下面的写作任务拟章节大纲，\
         每章一个对象：{{\"title\":\"章节标题\",\"brief\":\"这一章要写什么（一句话）\"}}。5-9 章为宜。\
         用与任务相同的语言。\n\n写作任务：{}",
        topic.trim()
    );
    let reply = crate::knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let json = crate::slides::extract_json(&reply).ok_or("回复里找不到大纲 JSON")?;
    let sections: Vec<WriteSection> =
        serde_json::from_str(&json).map_err(|e| format!("大纲解析失败: {e}"))?;
    if sections.is_empty() {
        return Err("生成的大纲是空的".to_string());
    }
    Ok(sections)
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WriteSection {
    pub title: String,
    #[serde(default)]
    pub brief: String,
}

/// AI 长文两阶段之二：按大纲逐章展开成 Markdown 正文。
#[tauri::command]
pub async fn write_expand_ai(
    topic: String,
    section: WriteSection,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let prompt = format!(
        "你是专业写作者。为整篇文章的其中一章撰写正文，只输出这一章的 Markdown（以 `## {}` 开头），\
         不要输出其他章节、不要解释。可以用要点列表和表格。用与任务相同的语言。\n\n\
         整篇文章的主题：{}\n本章要写：{}",
        section.title.trim(),
        topic.trim(),
        if section.brief.trim().is_empty() { section.title.trim() } else { section.brief.trim() }
    );
    crate::knowledge::chat_once(&db, &chat_model, &prompt).await
}

// ── P2 Excel + 统一预览 ─────────────────────────────────────────────────────

/// 任意 office 文件 → 自包含 HTML（表格工作台与工作区产物预览共用）。
#[tauri::command]
pub async fn office_preview_html(file_path: String) -> Result<String, String> {
    let lower = file_path.to_lowercase();
    if !(lower.ends_with(".docx") || lower.ends_with(".xlsx") || lower.ends_with(".pptx")) {
        return Err("只支持 .docx / .xlsx / .pptx".to_string());
    }
    office::preview_html(&file_path).await
}

/// 新建空白工作簿到导出目录。
#[tauri::command]
pub async fn excel_new(title: String) -> Result<String, String> {
    let safe = super::slides::sanitize_filename(if title.trim().is_empty() { "未命名表格" } else { title.trim() });
    let out = crate::storage::exports_dir().join(format!("{safe}.xlsx"));
    let out_str = out.to_string_lossy().to_string();
    if out.exists() {
        return Err(format!("已存在同名文件: {out_str}"));
    }
    let create = office::run(&["create", &out_str], 60).await?;
    if !create.status.success() {
        return Err(String::from_utf8_lossy(&create.stderr).trim().to_string());
    }
    Ok(out_str)
}

const EXCEL_BATCH_SPEC: &str = r#"你是表格助手，通过 officecli 的批量命令操作真实 xlsx。只输出一个 JSON 数组（不要解释、不要围栏），每个元素形如：
{"command":"set","path":"/Sheet1/B2","props":{"value":"120"}}
{"command":"set","path":"/Sheet1/C2","props":{"formula":"=SUM(B2:B9)"}}
{"command":"add","parent":"/","type":"sheet","props":{"name":"汇总"}}
规则：单元格路径 /工作表名/列行（如 /Sheet1/A1）；写公式用 formula（以 = 开头）；数字直接给 value；
可用 props 还有 bold、color（#RRGGBB）、align。不要删除或清空用户没提到的区域。"#;

/// 表格 AI 指令：当前表格文本快照 + 指令 → 批量命令 → 执行。
#[tauri::command]
pub async fn excel_ai_edit(
    file_path: String,
    instruction: String,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    if instruction.trim().is_empty() {
        return Err("请先输入指令".to_string());
    }
    let snapshot = office::xlsx_text(&file_path).await?;
    let prompt = format!(
        "{EXCEL_BATCH_SPEC}\n\n当前表格内容（A1=值 形式，空单元格省略）：\n{}\n\n用户指令：\n{}",
        snapshot.chars().take(6000).collect::<String>(),
        instruction.trim()
    );
    let reply = crate::knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let json = crate::slides::extract_json(&reply).ok_or("回复里找不到批量命令 JSON")?;
    office::apply_batch(&file_path, &json).await
}

/// CSV/TSV 导入到指定工作表（officecli 原生 import）。
#[tauri::command]
pub async fn excel_import_csv(file_path: String, csv_path: String) -> Result<(), String> {
    let output = office::run(&["import", &file_path, "/Sheet1", &csv_path], 180).await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

/// Import an existing .pptx into a new deck: OfficeCLI extracts per-slide text
/// and speaker notes; we map them onto the structured model so the whole
/// editing pipeline (大纲/精修/配图/撤销/导出) applies to decks made elsewhere.
///
/// v1 fidelity is deliberately text-first: layout is inferred (first slide →
/// cover; one line → section; otherwise bullets), images/charts are not carried
/// over — 配图 regenerates them. The import is snapshotted like any AI change.
#[tauri::command]
pub async fn import_pptx_deck(
    file_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<super::slides::DeckRecord, String> {
    if !file_path.to_lowercase().ends_with(".pptx") {
        return Err("只支持 .pptx 文件".to_string());
    }
    if !std::path::Path::new(&file_path).is_file() {
        return Err(format!("文件不存在: {file_path}"));
    }
    let extracted = office::extract_pptx_text(&file_path).await?;

    let stem = std::path::Path::new(&file_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "导入的演示".to_string());

    let slides: Vec<Slide> = extracted
        .iter()
        .enumerate()
        .map(|(i, ex)| {
            let mut lines = ex.lines.clone();
            let title = if lines.is_empty() { format!("第 {} 页", i + 1) } else { lines.remove(0) };
            let (layout, subtitle, bullets) = if i == 0 {
                // Cover: first remaining line becomes the subtitle.
                let subtitle = if lines.is_empty() { String::new() } else { lines.remove(0) };
                ("cover", subtitle, lines)
            } else if lines.is_empty() {
                ("section", String::new(), Vec::new())
            } else {
                ("bullets", String::new(), lines)
            };
            Slide {
                layout: layout.to_string(),
                title,
                subtitle,
                bullets,
                notes: ex.notes.clone(),
                ..Default::default()
            }
        })
        .collect();

    let deck = Deck {
        id: super::slides::make_id(),
        title: stem,
        theme: "midnight".to_string(),
        brand: None,
        slides,
    };
    let record = super::slides::persist_deck(&db, deck)?;
    super::slides::snapshot(&db, &record.id, "导入 pptx");
    Ok(record)
}
