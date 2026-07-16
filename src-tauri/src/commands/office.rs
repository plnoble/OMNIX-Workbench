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
