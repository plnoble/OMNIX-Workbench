//! Tauri commands for the PPT / presentation panel (user request #4).
//!
//! Deck persistence (SQLite `decks`), the canonical HTML render (preview ==
//! export), and AI generate/edit via the gateway model. The structured JSON
//! `Deck` is the single source of truth, so AI edits are surgical + the render
//! is deterministic.

use std::sync::Arc;

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;
use crate::knowledge;
use crate::slides::{self, Deck};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckMeta {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub slide_count: i64,
    pub updated_at: String,
}

/// A full deck as sent to the frontend. `model_json` is the serialized `Deck`
/// the editor mutates and passes back on save/render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckRecord {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub model_json: String,
}

pub(super) fn make_id() -> String {
    let nanos = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_millis() * 1_000_000);
    format!("deck_{nanos}_{}", std::process::id())
}

/// Serialize a `Deck` back to a JSON string, persist it, and return the record.
pub(super) fn persist_deck(db: &DbManager, mut deck: Deck) -> Result<DeckRecord, String> {
    if deck.id.is_empty() {
        deck.id = make_id();
    }
    // 每份存下去的 deck 都带齐控件默认值。放在这一个收口点，新建/生成/导入/
    // 回退/手改都自动覆盖到——旧 deck 打开一次也就补齐了，控件面板不会出现空值。
    for s in deck.slides.iter_mut() {
        s.fill_default_params();
    }
    let model_json = serde_json::to_string(&deck).map_err(|e| e.to_string())?;
    let count = deck.slides.len() as i64;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO decks (id, title, theme, model_json, slide_count, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
            title=excluded.title, theme=excluded.theme,
            model_json=excluded.model_json, slide_count=excluded.slide_count,
            updated_at=CURRENT_TIMESTAMP",
        params![deck.id, deck.title, deck.theme, model_json, count],
    )
    .map_err(|e| e.to_string())?;
    Ok(DeckRecord {
        id: deck.id,
        title: deck.title,
        theme: deck.theme,
        model_json,
    })
}

/// 读 deck，同时记下**读的那一刻**的内容指纹。
///
/// AI 编辑是「读 → 调模型（几秒到几十秒）→ 写回」。这中间前端的自动保存会把
/// 用户新打的字写进库，而写回用的是最初读到的底稿——用户在 AI 思考期间的编辑
/// 就被静默盖掉了。要挡住它，先得知道我们当初读的是哪一份。
fn read_deck_fingerprinted(db: &DbManager, id: &str) -> Result<(String, String), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let json: String = conn
        .query_row(
            "SELECT model_json FROM decks WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|_| "演示不存在".to_string())?;
    let fp = crate::skill_lock::sha256_hex(&json);
    Ok((json, fp))
}

/// 写回前确认库里还是我们读的那一份；变了就**不覆盖**。
///
/// 关键取舍：拒绝写入不能等于丢掉 AI 的结果——那只是把「丢用户的字」换成
/// 「丢模型的活」。所以冲突时把 AI 结果存进版本历史，用户可以从「撤销/版本」
/// 里取回来，两边都不丢。
fn persist_unless_changed(
    db: &DbManager,
    id: &str,
    deck: Deck,
    expected_fp: &str,
    what: &str,
) -> Result<DeckRecord, String> {
    let current: Option<String> = db
        .get_connection()
        .ok()
        .and_then(|c| {
            c.query_row(
                "SELECT model_json FROM decks WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .ok()
        });
    if let Some(cur) = current {
        if crate::skill_lock::sha256_hex(&cur) != expected_fp {
            // 先把 AI 的成果落到版本历史里，再报错——顺序反了就真丢了。
            if let (Ok(conn), Ok(json)) = (db.get_connection(), serde_json::to_string(&deck)) {
                let _ = conn.execute(
                    "INSERT INTO deck_versions (deck_id, model_json, label) VALUES (?1, ?2, ?3)",
                    params![id, json, format!("{what}（未应用）")],
                );
            }
            return Err(format!(
                "这份演示在 AI 处理期间被改动过，{what}是基于改动前的版本做的，已放弃写入以免覆盖你的改动。AI 的结果已存进版本历史，可以在「撤销」旁边的版本列表里取回。"
            ));
        }
    }
    persist_deck(db, deck)
}

/// Snapshot the deck's CURRENT stored model before an AI mutation overwrites it,
/// so any AI edit is undoable. Keeps the newest 20 versions per deck.
/// Best-effort: a snapshot failure must never block the edit itself.
pub(super) fn snapshot(db: &DbManager, deck_id: &str, label: &str) {
    let Ok(conn) = db.get_connection() else { return };
    let current: Option<String> = conn
        .query_row(
            "SELECT model_json FROM decks WHERE id = ?1",
            params![deck_id],
            |r| r.get(0),
        )
        .ok();
    let Some(json) = current else { return };
    let _ = conn.execute(
        "INSERT INTO deck_versions (deck_id, model_json, label) VALUES (?1, ?2, ?3)",
        params![deck_id, json, label],
    );
    let _ = conn.execute(
        "DELETE FROM deck_versions WHERE deck_id = ?1 AND id NOT IN
           (SELECT id FROM deck_versions WHERE deck_id = ?1 ORDER BY id DESC LIMIT 20)",
        params![deck_id],
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckVersion {
    pub id: i64,
    pub label: String,
    pub created_at: String,
}

#[tauri::command]
pub fn list_deck_versions(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<DeckVersion>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, label, created_at FROM deck_versions WHERE deck_id = ?1 ORDER BY id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(DeckVersion { id: r.get(0)?, label: r.get(1)?, created_at: r.get(2)? })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// Restore a snapshot (or the latest one when `version_id` is None = plain undo).
/// The pre-restore state is itself snapshotted, so undo is undoable.
#[tauri::command]
pub fn restore_deck_version(
    id: String,
    version_id: Option<i64>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let (vid, json): (i64, String) = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        match version_id {
            Some(v) => conn.query_row(
                "SELECT id, model_json FROM deck_versions WHERE deck_id = ?1 AND id = ?2",
                params![id, v],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ),
            None => conn.query_row(
                "SELECT id, model_json FROM deck_versions WHERE deck_id = ?1 ORDER BY id DESC LIMIT 1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ),
        }
        .map_err(|_| "没有可回退的版本".to_string())?
    };
    snapshot(&db, &id, "回退前");
    let mut deck: Deck = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    deck.id = id.clone();
    let rec = persist_deck(&db, deck)?;
    // The restored snapshot is now the live state — drop it from history.
    if let Ok(conn) = db.get_connection() {
        let _ = conn.execute("DELETE FROM deck_versions WHERE id = ?1", params![vid]);
    }
    Ok(rec)
}

#[tauri::command]
pub fn list_decks(db: State<'_, Arc<DbManager>>) -> Result<Vec<DeckMeta>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, theme, slide_count, updated_at
             FROM decks ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DeckMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                theme: r.get(2)?,
                slide_count: r.get(3)?,
                updated_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_deck(id: String, db: State<'_, Arc<DbManager>>) -> Result<DeckRecord, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, title, theme, model_json FROM decks WHERE id = ?1",
        params![id],
        |r| {
            Ok(DeckRecord {
                id: r.get(0)?,
                title: r.get(1)?,
                theme: r.get(2)?,
                model_json: r.get(3)?,
            })
        },
    )
    .map_err(|_| "演示不存在".to_string())
}

#[tauri::command]
pub fn create_deck(
    title: String,
    theme: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let title = if title.trim().is_empty() {
        "未命名演示".to_string()
    } else {
        title
    };
    let theme = if slides::THEMES.contains(&theme.as_str()) {
        theme
    } else {
        "midnight".to_string()
    };
    let deck = Deck {
        id: make_id(),
        title: title.clone(),
        theme,
        brand: None,
        slides: vec![slides::Slide {
            layout: "cover".to_string(),
            title,
            subtitle: "用下方指令让 AI 生成，或直接编辑".to_string(),
            ..Default::default()
        }],
    };
    persist_deck(&db, deck)
}

/// Save an edited deck. The incoming `model_json` is parsed (validated) before
/// persisting so a malformed edit can never corrupt the stored deck.
#[tauri::command]
pub fn save_deck(
    id: String,
    model_json: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let mut deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    deck.id = id;
    if deck.slides.is_empty() {
        return Err("演示至少需要一页".to_string());
    }
    persist_deck(&db, deck)
}

#[tauri::command]
pub fn delete_deck(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM decks WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Render a deck (or one slide) to a self-contained HTML document. Used both for
/// the live preview (iframe srcdoc) and for export. No DB access — pure.
#[tauri::command]
pub fn render_deck(
    model_json: String,
    slide_index: Option<usize>,
    print: bool,
) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    Ok(slides::render_deck_html(&deck, slide_index, print))
}

fn exports_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::storage::exports_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Keep deck titles usable as file names.
pub(super) fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "presentation".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Reveal the exported file in Explorer / file manager (GUI launcher — no console).
pub(super) fn reveal_in_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

/// Export the deck as a self-contained HTML file into ~/.omnix/exports.
///
/// 导出的文件里**嵌了一份 deck JSON**，所以它不是死路：`import_deck_html`
/// 能原样读回来继续编辑。以前导出等于把结构化模型压成一堆 div，改一个字
/// 都得回来重做。
#[tauri::command]
pub fn export_deck_html(model_json: String) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    let html = crate::slides::render_deck_html(&deck, None, true);
    let html = crate::slides::embed_deck_source(&html, &deck)?;
    let path = exports_dir()?.join(format!("{}.html", sanitize_filename(&deck.title)));
    std::fs::write(&path, html).map_err(|e| format!("写出 HTML 失败: {e}"))?;
    reveal_in_folder(&path);
    Ok(path.to_string_lossy().to_string())
}

/// 把导出的 HTML 读回成一份可编辑的演示（P3 往返）。
/// 只认自己导出的文件——别人的 HTML 里没有那个数据块，明确报错而不是瞎猜。
#[tauri::command]
pub fn import_deck_html(
    file_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    crate::input_validation::validate_user_file_path(&file_path, "file_path")?;
    let html = std::fs::read_to_string(&file_path).map_err(|e| format!("读取失败: {e}"))?;
    let json = crate::slides::extract_deck_source(&html).ok_or_else(|| {
        "这个 HTML 里没有 OMNIX 的演示数据块——只有从 OMNIX 导出的 HTML 才能导回。".to_string()
    })?;
    let mut deck: Deck =
        serde_json::from_str(&json).map_err(|e| format!("演示数据损坏: {e}"))?;
    if deck.slides.is_empty() {
        return Err("这份演示没有任何幻灯页".to_string());
    }
    // 新 id：导回来是「另存一份」，不覆盖库里可能还在的原稿。
    deck.id = make_id();
    persist_deck(&db, deck)
}

/// 打开演讲者视图窗口（P2）。
///
/// 必须是**另一个窗口**：备注只能给讲的人看。同一块屏幕上放备注等于把小抄
/// 投给全场——那样的"演讲者视图"不如不做。第二块屏拖过去全屏即可。
///
/// 幂等：已经开着就聚焦，不重复建窗。
#[tauri::command]
pub fn open_speaker_view(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(SPEAKER_WINDOW) {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        SPEAKER_WINDOW,
        tauri::WebviewUrl::App(format!("/?window={SPEAKER_WINDOW}").into()),
    )
    .title("OMNIX 演讲者视图")
    .inner_size(1100.0, 700.0)
    .build()
    .map_err(|e| format!("创建演讲者视图失败: {e}"))?;
    Ok(())
}

/// 窗口 label —— 必须同时出现在 `capabilities/default.json` 的 windows 列表里，
/// 否则新窗口拿不到任何权限，invoke 会被直接拒掉（是个很难查的静默失败）。
pub const SPEAKER_WINDOW: &str = "slides-speaker";

#[tauri::command]
pub fn close_speaker_view(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(SPEAKER_WINDOW) {
        let _ = w.close();
    }
    Ok(())
}

/// 体检（P0）：把渲染时会静默咽下去的问题一次报出来。纯函数，不碰 DB。
#[tauri::command]
pub fn lint_deck(model_json: String) -> Result<crate::slides_lint::LintReport, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    Ok(crate::slides_lint::lint_deck(&deck))
}

#[cfg(windows)]
fn find_edge() -> Option<std::path::PathBuf> {
    for base in [
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("ProgramFiles").ok(),
    ]
    .into_iter()
    .flatten()
    {
        let p = std::path::PathBuf::from(base).join("Microsoft/Edge/Application/msedge.exe");
        if p.exists() {
            return Some(p);
        }
    }
    which::which("msedge").ok()
}

/// Export the deck as a real PDF using Edge/Chrome headless printing (Windows
/// ships Edge, so no extra dependency). Falls back with a clear error telling
/// the user to export HTML and print manually if no browser is found.
#[tauri::command]
pub async fn export_deck_pdf(model_json: String) -> Result<String, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    let html = crate::slides::render_deck_html(&deck, None, true);
    let dir = exports_dir()?;
    let stem = sanitize_filename(&deck.title);
    let html_path = dir.join(format!(".{stem}.print.html"));
    let pdf_path = dir.join(format!("{stem}.pdf"));
    std::fs::write(&html_path, html).map_err(|e| format!("写出临时 HTML 失败: {e}"))?;

    #[cfg(windows)]
    let browser = find_edge();
    #[cfg(not(windows))]
    let browser = which::which("google-chrome")
        .or_else(|_| which::which("chromium"))
        .ok();

    let Some(browser) = browser else {
        let _ = std::fs::remove_file(&html_path);
        return Err("没找到 Edge/Chrome，无法直接导出 PDF——请改用「导出 HTML」后在浏览器里打印为 PDF".into());
    };

    let file_url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    let mut cmd = tokio::process::Command::new(&browser);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-pdf-header-footer")
        // 1280×720 slide → landscape page matching the 16:9 canvas
        .arg(format!("--print-to-pdf={}", pdf_path.to_string_lossy()))
        .arg(&file_url);
    crate::proc::NoWindow::no_window(&mut cmd);
    let status = cmd
        .status()
        .await
        .map_err(|e| format!("启动浏览器打印失败: {e}"))?;
    let _ = std::fs::remove_file(&html_path);
    if !status.success() || !pdf_path.exists() {
        return Err("浏览器打印 PDF 失败——请改用「导出 HTML」后手动打印".into());
    }
    reveal_in_folder(&pdf_path);
    Ok(pdf_path.to_string_lossy().to_string())
}

/// Generate a brand-new deck from a topic via the gateway model, persist it.
#[tauri::command]
pub async fn generate_deck(
    topic: String,
    chat_model: String,
    slide_count: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if topic.trim().is_empty() {
        return Err("请先描述要做什么演示".to_string());
    }
    let prompt = slides::build_generate_prompt(topic.trim(), slide_count.unwrap_or(10));
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let deck = slides::parse_deck(&reply)?;
    persist_deck(&db, deck)
}

// ── P2：版式目录（角色 + 控件契约）──────────────────────────────────────────

/// 前端控件面板的数据源。**契约只有后端一份**——控件的取值范围、默认值、
/// 选项都由 `slides_layout` 说了算，前端只负责画滑杆和开关。这样加一个新
/// 控件不需要改两处，也不会出现「前端允许拖到 10、后端夹到 6」的错位。
#[derive(Debug, Clone, Serialize)]
pub struct LayoutInfo {
    pub key: &'static str,
    pub label: &'static str,
    /// 这个版式内容该往哪些字段放（编辑器提示用）
    pub fields_hint: &'static str,
    pub controls: Vec<crate::slides_layout::Control>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutCatalog {
    pub roles: &'static [crate::slides_layout::PageRole],
    pub layouts: Vec<LayoutInfo>,
}

#[tauri::command]
pub fn slides_layout_catalog() -> LayoutCatalog {
    use crate::slides_layout as L;
    LayoutCatalog {
        roles: L::PAGE_ROLES,
        layouts: L::ALL_LAYOUTS
            .iter()
            .map(|k| LayoutInfo {
                key: k,
                label: L::layout_label(k),
                fields_hint: L::fields_hint_for(k),
                controls: L::controls_for(k),
            })
            .collect(),
    }
}

// ── A：两阶段生成（大纲 → 展开）─────────────────────────────────────────────

/// Stage 1: plan the deck as an outline the user can fix in seconds before any
/// expensive full generation happens.
#[tauri::command]
pub async fn generate_outline(
    topic: String,
    chat_model: String,
    slide_count: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<slides::Outline, String> {
    if topic.trim().is_empty() {
        return Err("请先描述要做什么演示".to_string());
    }
    let prompt = slides::build_outline_prompt(topic.trim(), slide_count.unwrap_or(10));
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let json = slides::extract_json(&reply).ok_or("回复里找不到大纲 JSON")?;
    let mut outline: slides::Outline =
        serde_json::from_str(&json).map_err(|e| format!("大纲 JSON 解析失败: {e}"))?;
    if outline.items.is_empty() {
        return Err("生成的大纲是空的".to_string());
    }
    outline.normalize();
    Ok(outline)
}

/// Stage 2: expand each outline item into a full slide, **in parallel**, then
/// persist as a new deck. A page that fails to parse degrades to its outline
/// content instead of sinking the whole deck.
#[tauri::command]
pub async fn expand_outline(
    mut outline: slides::Outline,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if outline.items.is_empty() {
        return Err("大纲是空的".to_string());
    }
    // 大纲可能被前端改过，这里再兜一次底：版式必须是认识的，否则按角色推导。
    outline.normalize();
    let total = outline.items.len();
    let title = outline.title.clone();

    let mut tasks = Vec::new();
    for (i, item) in outline.items.iter().enumerate() {
        let prompt = slides::build_expand_slide_prompt(&title, i, total, item);
        let model = chat_model.clone();
        let db2 = Arc::clone(&db);
        let fallback = item.clone();
        tasks.push(async move {
            match knowledge::chat_once(&db2, &model, &prompt).await {
                Ok(reply) => slides::extract_json(&reply)
                    .and_then(|j| serde_json::from_str::<slides::Slide>(&j).ok())
                    .unwrap_or_else(|| fallback_slide(&fallback)),
                Err(_) => fallback_slide(&fallback),
            }
        });
    }
    let mut expanded: Vec<slides::Slide> = futures::future::join_all(tasks).await;
    // 角色决定版式（模型漏填或编造 layout 时兜底）。控件默认值由 persist_deck 统一补。
    for (slide, item) in expanded.iter_mut().zip(outline.items.iter()) {
        if slide.role.is_empty() {
            slide.role = item.role.clone();
        }
        if slide.layout.is_empty() && !slide.role.is_empty() {
            slide.layout = crate::slides_layout::default_layout_for_role(&slide.role).to_string();
        }
    }

    let deck = Deck {
        id: make_id(),
        title: outline.title,
        theme: outline.theme,
        brand: None,
        slides: expanded,
    };
    persist_deck(&db, deck)
}

/// Outline item → a usable slide when the model call/parse fails.
fn fallback_slide(item: &slides::OutlineItem) -> slides::Slide {
    slides::Slide {
        layout: item.layout.clone(),
        title: item.title.clone(),
        bullets: item.points.clone(),
        ..Default::default()
    }
}

// ── B：单页精修（差分编辑）──────────────────────────────────────────────────

/// Edit ONE slide: only that slide's JSON goes to the model and only that slide
/// comes back. Much faster/cheaper than a whole-deck round trip, and it
/// physically cannot corrupt other pages.
///
/// `lock_template`（默认 true）= 只换文案，不动版式/角色/控件参数/图片槽。
/// 提示词里也写了这条，但光靠提示词约束模型不可靠——解析后会确定性还原这些
/// 字段，所以精心调好的版式不会被一句「文字再精炼点」顺手改掉。
#[tauri::command]
pub async fn edit_slide_ai(
    id: String,
    slide_index: usize,
    instruction: String,
    chat_model: String,
    lock_template: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if instruction.trim().is_empty() {
        return Err("请先输入修改指令".to_string());
    }
    // S0：记下读的那一刻的指纹，写回前比对（见 persist_unless_changed）
    let (current, fingerprint) = read_deck_fingerprinted(&db, &id)?;
    let deck_id_for_guard = id.clone();
    let mut deck: Deck = serde_json::from_str(&current).map_err(|e| e.to_string())?;
    let slide = deck
        .slides
        .get(slide_index)
        .ok_or_else(|| "页码超出范围".to_string())?;
    let slide_json = serde_json::to_string_pretty(slide).map_err(|e| e.to_string())?;
    let prompt = slides::build_slide_edit_prompt(
        &deck.title,
        slide_index,
        deck.slides.len(),
        &slide_json,
        instruction.trim(),
    );
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    snapshot(&db, &id, &format!("AI 改第 {} 页前", slide_index + 1));
    let json = slides::extract_json(&reply).ok_or("回复里找不到这一页的 JSON")?;
    let mut new_slide: slides::Slide =
        serde_json::from_str(&json).map_err(|e| format!("单页 JSON 解析失败: {e}"))?;
    if lock_template.unwrap_or(true) {
        // 锁模板：模型改了也还原。保住的是精心设计的版式，模型只被允许改文案。
        let original = deck.slides[slide_index].clone();
        new_slide.restore_template_from(&original);
    }
    new_slide.fill_default_params();
    deck.slides[slide_index] = new_slide;
    deck.id = id;
    persist_unless_changed(&db, &deck_id_for_guard, deck, &fingerprint, "AI 的单页修改")
}

// ── 每页多候选：先给不要钱的，AI 只出一个 ───────────────────────────────────

/// 一个候选方案。`html` 是这一页的完整渲染，前端直接塞 iframe 对比。
#[derive(Debug, Clone, Serialize)]
pub struct SlideCandidate {
    pub label: String,
    /// `template` = 本地重排（秒出、免费）；`ai` = 模型重构
    pub kind: &'static str,
    pub slide_json: String,
    pub html: String,
}

/// 本地能算出的候选：`(标签, 幻灯)`。不碰 DB、不调模型，所以可以直接测。
/// 第一项恒为「当前」，方便前端把原状放在第一格做对照。
pub(super) fn template_candidates(current: &slides::Slide) -> Vec<(String, slides::Slide)> {
    let mut out = vec![("当前".to_string(), current.clone())];
    // 按**渲染结果**去重：换了参数不等于看起来不一样。比如指标卡只有 3 条数据时，
    // 「指标数」从 3 拖到 4 什么也不会变；那种候选摆出来是在浪费用户的一次点击。
    let mut seen = std::collections::HashSet::from([slides::render_slide_fragment(current)]);
    let mut push = |label: String, s: slides::Slide, out: &mut Vec<_>| {
        if seen.insert(slides::render_slide_fragment(&s)) {
            out.push((label, s));
        }
    };

    // 1) 同版式换排布
    let mut tweaked = current.clone();
    tweaked.params = crate::slides_layout::variant_params(&tweaked.layout, &tweaked.params);
    push("换个排布".to_string(), tweaked, &mut out);

    // 2) 换版式（角色推荐的兄弟版式）
    for alt in crate::slides_layout::alternative_layouts(
        &current.role,
        &current.layout,
        !current.items.is_empty(),
        !current.columns.is_empty(),
    ) {
        let mut s = current.clone();
        s.layout = alt.to_string();
        s.fill_default_params();
        push(
            format!("改用「{}」", crate::slides_layout::layout_label(alt)),
            s,
            &mut out,
        );
    }
    out
}

/// 给这一页出几个候选：**模板候选是本地算的**（换版式、换排布），只有最后一个
/// 才调模型。多数时候用户想要的只是「换个样子」，那不该花一次模型调用。
///
/// AI 候选失败不影响其他候选——拿不到就少一个选项，不是整个功能报错。
#[tauri::command]
pub async fn slide_candidates(
    id: String,
    slide_index: usize,
    chat_model: String,
    include_ai: Option<bool>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SlideCandidate>, String> {
    let deck: Deck = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let json = conn
            .query_row("SELECT model_json FROM decks WHERE id = ?1", params![id], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|_| "演示不存在".to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())?
    };
    let current = deck
        .slides
        .get(slide_index)
        .ok_or_else(|| "页码超出范围".to_string())?
        .clone();

    // 渲染时把候选放回原 deck，主题/母版/页码都保持真实上下文。
    let render = |s: &slides::Slide| -> String {
        let mut d = deck.clone();
        d.slides[slide_index] = s.clone();
        slides::render_deck_html(&d, Some(slide_index), false)
    };
    let mut out: Vec<SlideCandidate> = template_candidates(&current)
        .into_iter()
        .map(|(label, s)| SlideCandidate {
            label,
            kind: "template",
            slide_json: serde_json::to_string(&s).unwrap_or_default(),
            html: render(&s),
        })
        .collect();

    // 3) AI 候选：唯一一次模型调用，且**允许**它换结构（与模板锁相反的那一档）
    if include_ai.unwrap_or(true) && !chat_model.trim().is_empty() {
        let slide_json = serde_json::to_string_pretty(&current).unwrap_or_default();
        let prompt = slides::build_slide_edit_prompt(
            &deck.title,
            slide_index,
            deck.slides.len(),
            &slide_json,
            "请重新设计这一页：可以换 layout、重组内容结构，目标是让信息更好读、更有说服力。内容含义不变。",
        );
        if let Ok(reply) = knowledge::chat_once(&db, &chat_model, &prompt).await {
            if let Some(mut s) = slides::extract_json(&reply)
                .and_then(|j| serde_json::from_str::<slides::Slide>(&j).ok())
            {
                s.fill_default_params();
                out.push(SlideCandidate {
                    label: "AI 重构".to_string(),
                    kind: "ai",
                    slide_json: serde_json::to_string(&s).unwrap_or_default(),
                    html: render(&s),
                });
            }
        }
    }
    Ok(out)
}

/// 采用一个候选。和别的 AI 改动一样先快照，选错了可以撤销。
#[tauri::command]
pub fn apply_slide_candidate(
    id: String,
    slide_index: usize,
    slide_json: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let mut slide: slides::Slide =
        serde_json::from_str(&slide_json).map_err(|e| format!("候选 JSON 无效: {e}"))?;
    let current = {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        conn.query_row("SELECT model_json FROM decks WHERE id = ?1", params![id], |r| {
            r.get::<_, String>(0)
        })
        .map_err(|_| "演示不存在".to_string())?
    };
    let mut deck: Deck = serde_json::from_str(&current).map_err(|e| e.to_string())?;
    if slide_index >= deck.slides.len() {
        return Err("页码超出范围".to_string());
    }
    snapshot(&db, &id, &format!("换第 {} 页方案前", slide_index + 1));
    slide.fill_default_params();
    deck.slides[slide_index] = slide;
    deck.id = id;
    persist_deck(&db, deck)
}

// ── C：自动配图 ─────────────────────────────────────────────────────────────

/// Suggested image prompt for a slide (local, no model call).
#[tauri::command]
pub fn suggest_slide_image_prompt(
    model_json: String,
    slide_index: usize,
) -> Result<String, String> {
    let deck: Deck = serde_json::from_str(&model_json).map_err(|e| e.to_string())?;
    let slide = deck.slides.get(slide_index).ok_or("页码超出范围")?;
    Ok(slides::build_image_prompt(slide, &deck.title))
}

/// Generate an illustration through the existing media pipeline and write the
/// resulting local path into `slide.image` (renderer inlines it as a data URI,
/// so preview/HTML/PDF/pptx all stay self-contained).
#[tauri::command]
pub async fn generate_slide_image(
    id: String,
    slide_index: usize,
    platform_id: String,
    model: String,
    prompt: String,
    size: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    let task = crate::commands::media_generate_image_core(
        &db,
        &platform_id,
        &model,
        prompt.trim(),
        size.as_deref().unwrap_or("1280x720"),
    )
    .await?;
    let path = task
        .result_path
        .ok_or_else(|| task.error.unwrap_or_else(|| "生图未返回结果".to_string()))?;

    // S0：记下读的那一刻的指纹，写回前比对（见 persist_unless_changed）
    let (current, fingerprint) = read_deck_fingerprinted(&db, &id)?;
    let deck_id_for_guard = id.clone();
    let mut deck: Deck = serde_json::from_str(&current).map_err(|e| e.to_string())?;
    snapshot(&db, &id, &format!("配图第 {} 页前", slide_index + 1));
    let slide = deck
        .slides
        .get_mut(slide_index)
        .ok_or_else(|| "页码超出范围".to_string())?;
    slide.image = path;
    // A text-only layout won't show the picture — promote it so the image lands.
    if slide.layout == "bullets" || slide.layout == "cover" || slide.layout == "section" {
        slide.layout = "image-left".to_string();
    }
    deck.id = id;
    persist_unless_changed(&db, &deck_id_for_guard, deck, &fingerprint, "生成的配图")
}

// ── D：母版 / 品牌 ──────────────────────────────────────────────────────────

/// Reusable brand masters (saved separately from any one deck).
#[tauri::command]
pub fn list_brands(db: State<'_, Arc<DbManager>>) -> Result<Vec<slides::Brand>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT brand_json FROM deck_brands ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows
        .flatten()
        .filter_map(|j| serde_json::from_str::<slides::Brand>(&j).ok())
        .collect())
}

#[tauri::command]
pub fn save_brand(brand: slides::Brand, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    if brand.name.trim().is_empty() {
        return Err("母版需要一个名字".to_string());
    }
    let json = serde_json::to_string(&brand).map_err(|e| e.to_string())?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO deck_brands (name, brand_json, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(name) DO UPDATE SET brand_json=excluded.brand_json, updated_at=CURRENT_TIMESTAMP",
        params![brand.name, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_brand(name: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM deck_brands WHERE name = ?1", params![name])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── E：导出 .pptx ───────────────────────────────────────────────────────────

/// Export a real PowerPoint file from the same JSON model.
#[tauri::command]
pub async fn export_deck_pptx(model_json: String) -> Result<PptxExportResult, String> {
    let deck: Deck =
        serde_json::from_str(&model_json).map_err(|e| format!("演示 JSON 无效: {e}"))?;
    let bytes = crate::pptx::build_pptx(&deck)?;
    let path = exports_dir()?.join(format!("{}.pptx", sanitize_filename(&deck.title)));
    std::fs::write(&path, bytes).map_err(|e| format!("写出 pptx 失败: {e}"))?;
    // 质检门：OfficeCLI schema 校验 + 内容问题扫描。导出永不因质检失败——
    // officecli 缺席时 qa.ran = false，前端提示“未质检”。
    let qa = crate::office::pptx_qa(&path.to_string_lossy()).await;
    reveal_in_folder(&path);
    Ok(PptxExportResult { path: path.to_string_lossy().to_string(), qa })
}

#[derive(Debug, Clone, Serialize)]
pub struct PptxExportResult {
    pub path: String,
    pub qa: crate::office::PptxQa,
}

/// AI-edit an existing deck with a natural-language instruction. Loads the
/// current model, asks the model to change only what's needed, re-persists.
#[tauri::command]
pub async fn edit_deck_ai(
    id: String,
    instruction: String,
    chat_model: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<DeckRecord, String> {
    if instruction.trim().is_empty() {
        return Err("请先输入修改指令".to_string());
    }
    // S0：记下读的那一刻的指纹，写回前比对（见 persist_unless_changed）
    let (current, fingerprint) = read_deck_fingerprinted(&db, &id)?;
    let deck_id_for_guard = id.clone();
    let prompt = slides::build_edit_prompt(&current, instruction.trim());
    let reply = knowledge::chat_once(&db, &chat_model, &prompt).await?;
    let mut deck = slides::parse_deck(&reply)?;
    snapshot(&db, &id, "AI 改整份前");
    deck.id = id;
    persist_unless_changed(&db, &deck_id_for_guard, deck, &fingerprint, "AI 的整份修改")
}

#[cfg(test)]
mod candidate_tests {
    use super::*;

    fn slide(layout: &str, role: &str) -> slides::Slide {
        let mut s = slides::Slide {
            layout: layout.into(),
            role: role.into(),
            title: "标题".into(),
            bullets: vec!["一".into(), "二".into(), "三".into()],
            ..Default::default()
        };
        s.fill_default_params();
        s
    }

    /// 候选的全部价值在于「不一样」。给出两个渲染一致的方案等于没给。
    #[test]
    fn candidates_are_all_distinct() {
        for (layout, role) in [
            ("bullets", "agenda"),
            ("metrics", "metric"),
            ("swot", "matrix"),
            ("chart", "trend"),
            ("content", ""),
        ] {
            let cur = slide(layout, role);
            let cands = template_candidates(&cur);
            assert!(cands.len() >= 2, "{layout} 只出了 {} 个候选", cands.len());
            assert_eq!(cands[0].0, "当前", "第一格必须是原状");

            let mut seen = std::collections::HashSet::new();
            for (label, s) in &cands {
                let html = slides::render_deck_html(
                    &slides::Deck {
                        id: String::new(),
                        title: "T".into(),
                        theme: "midnight".into(),
                        brand: None,
                        slides: vec![s.clone()],
                    },
                    None,
                    false,
                );
                assert!(seen.insert(html), "{layout} 的候选「{label}」跟前面某个渲染一样");
            }
        }
    }

    /// 版式目录的 JSON 形状是前后端唯一的契约，前端照着它画控件面板。
    /// 字段名或枚举写法一变，面板就会静默变空——TS 那边只有 interface，
    /// 编译期查不出来。这条用例把形状钉死在后端。
    #[test]
    fn layout_catalog_json_matches_the_frontend_contract() {
        let v = serde_json::to_value(slides_layout_catalog()).unwrap();

        let role = &v["roles"][0];
        for k in ["key", "label", "layouts", "intent"] {
            assert!(!role[k].is_null(), "roles[] 缺字段 {k}");
        }
        assert!(role["layouts"].is_array());

        let layouts = v["layouts"].as_array().unwrap();
        assert_eq!(layouts.len(), crate::slides_layout::ALL_LAYOUTS.len());
        for l in layouts {
            for k in ["key", "label", "fields_hint", "controls"] {
                assert!(!l[k].is_null(), "layouts[] 缺字段 {k}");
            }
            for c in l["controls"].as_array().unwrap() {
                for k in ["key", "label", "kind", "default", "desc"] {
                    assert!(!c[k].is_null(), "controls[] 缺字段 {k}（版式 {}）", l["key"]);
                }
                // TS: type ControlKind = "range" | "toggle" | "select"
                let kind = c["kind"].as_str().unwrap();
                assert!(
                    ["range", "toggle", "select"].contains(&kind),
                    "控件类型 {kind} 不在前端的联合类型里"
                );
                match kind {
                    "range" => {
                        assert!(c["min"].is_i64() && c["max"].is_i64(), "range 缺 min/max");
                        assert!(c["default"].is_i64(), "range 的默认值应是整数");
                    }
                    "toggle" => assert!(c["default"].is_boolean(), "toggle 的默认值应是布尔"),
                    _ => {
                        let opts = c["options"].as_array().expect("select 必须带 options");
                        assert!(opts.len() >= 2, "select 少于两个选项没有意义");
                        // TS: options?: [string, string][]
                        for o in opts {
                            let pair = o.as_array().unwrap();
                            assert_eq!(pair.len(), 2, "选项应是 [key, 中文名]");
                        }
                        let def = c["default"].as_str().expect("select 的默认值应是字符串");
                        assert!(
                            opts.iter().any(|o| o[0] == def),
                            "select 的默认值 {def} 不在自己的选项里"
                        );
                    }
                }
            }
        }
    }

    /// 候选只换呈现，不改文案——用户点「换方案」不该发现字被改了。
    #[test]
    fn candidates_never_touch_the_text() {
        let cur = slide("bullets", "agenda");
        for (label, s) in template_candidates(&cur) {
            assert_eq!(s.title, cur.title, "候选「{label}」改了标题");
            assert_eq!(s.bullets, cur.bullets, "候选「{label}」改了要点");
            assert_eq!(s.body, cur.body, "候选「{label}」改了正文");
        }
    }
}

#[cfg(test)]
mod concurrent_edit_tests {
    use super::*;

    fn db(tag: &str) -> Arc<DbManager> {
        let p = std::env::temp_dir().join(format!("omnix_ce_{}_{tag}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        Arc::new(DbManager::new_with_path(p))
    }

    fn seed(db: &DbManager, id: &str, title: &str) -> String {
        let deck = Deck {
            id: id.into(),
            title: title.into(),
            theme: "midnight".into(),
            brand: None,
            slides: vec![slides::Slide { title: title.into(), ..Default::default() }],
        };
        persist_deck(db, deck).unwrap().model_json
    }

    /// 没人动过就正常写回——护栏不能把正常路径也挡了。
    #[test]
    fn untouched_deck_persists_normally() {
        let d = db("ok");
        seed(&d, "k1", "原始");
        let (json, fp) = read_deck_fingerprinted(&d, "k1").unwrap();
        let mut deck: Deck = serde_json::from_str(&json).unwrap();
        deck.title = "AI 改过的".into();
        let rec = persist_unless_changed(&d, "k1", deck, &fp, "AI 的修改").unwrap();
        assert!(rec.model_json.contains("AI 改过的"));
    }

    /// 核心承诺：AI 思考期间用户改了字，那些字不能被盖掉。
    #[test]
    fn user_edits_during_the_ai_call_are_not_clobbered() {
        let d = db("clobber");
        seed(&d, "k2", "原始");
        // AI 读走底稿
        let (json, fp) = read_deck_fingerprinted(&d, "k2").unwrap();
        let mut ai_result: Deck = serde_json::from_str(&json).unwrap();
        ai_result.title = "AI 写的标题".into();

        // 模型还在跑的时候，用户打了字（前端自动保存）
        seed(&d, "k2", "用户打的字");

        let err = persist_unless_changed(&d, "k2", ai_result, &fp, "AI 的修改").unwrap_err();
        assert!(err.contains("放弃写入"), "{err}");

        // 库里必须还是用户的版本
        let (after, _) = read_deck_fingerprinted(&d, "k2").unwrap();
        assert!(after.contains("用户打的字"), "用户的改动被盖掉了：{after}");
        assert!(!after.contains("AI 写的标题"));
    }

    /// 拒绝写入不能等于丢掉 AI 的活——那只是把「丢用户的字」换成「丢模型的活」。
    #[test]
    fn the_rejected_ai_result_is_kept_in_version_history() {
        let d = db("keep");
        seed(&d, "k3", "原始");
        let (json, fp) = read_deck_fingerprinted(&d, "k3").unwrap();
        let mut ai_result: Deck = serde_json::from_str(&json).unwrap();
        ai_result.title = "AI 写的标题".into();
        seed(&d, "k3", "用户打的字");

        let _ = persist_unless_changed(&d, "k3", ai_result, &fp, "AI 的修改");

        let conn = d.get_connection().unwrap();
        let (label, json): (String, String) = conn
            .query_row(
                "SELECT label, model_json FROM deck_versions WHERE deck_id='k3' ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("AI 结果必须留在版本历史里");
        assert!(label.contains("未应用"), "标签要说清它没被应用: {label}");
        assert!(json.contains("AI 写的标题"), "存的必须是 AI 的结果");
    }

    /// 指纹要认得出内容变化，而不是只看时间戳之类的东西。
    #[test]
    fn fingerprint_tracks_content_not_timestamps() {
        let d = db("fp");
        seed(&d, "k4", "内容");
        let (_, fp1) = read_deck_fingerprinted(&d, "k4").unwrap();
        // 原样再存一次：内容没变，指纹必须一样
        seed(&d, "k4", "内容");
        let (_, fp2) = read_deck_fingerprinted(&d, "k4").unwrap();
        assert_eq!(fp1, fp2, "内容没变，指纹不该变");
        seed(&d, "k4", "别的内容");
        let (_, fp3) = read_deck_fingerprinted(&d, "k4").unwrap();
        assert_ne!(fp1, fp3);
    }
}
