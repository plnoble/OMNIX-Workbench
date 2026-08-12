//! Notes (笔记) — lightweight local Markdown notes. Notes can be created anywhere, and the Quick
//! Assistant can save a result/selection straight into one (`source` records
//! where it came from). Stored locally in SQLite; content is plain Markdown.

use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbManager;

/// 笔记会镜像成真正的 Markdown 文件落盘（便携，也让基于文件的 agent 能读）。
///
/// 目录走「设置 → 存储位置」，默认 `~/.omnix/notes/`。以前这里写死 home 目录，
/// 笔记只能待在 C 盘——而存储位置中心本来就是为了把会长大的目录挪走建的，
/// 笔记只是漏了没接进去。
fn notes_dir() -> Option<PathBuf> {
    let dir = crate::storage::notes_dir();
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

fn write_note_file(note: &Note) {
    if let Some(dir) = notes_dir() {
        let body = format!("# {}\n\n{}\n", note.title, note.content);
        let _ = std::fs::write(dir.join(format!("{}.md", note.id)), body);
    }
}

fn delete_note_file(id: &str) {
    if let Some(dir) = notes_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{}.md", id)));
    }
}

/// Absolute path of the on-disk notes folder (for an "open folder" action).
#[tauri::command]
pub fn get_notes_dir() -> Result<String, String> {
    notes_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "无法定位笔记目录".into())
}

/// Open the notes folder in the OS file manager. Uses the native command
/// directly so it never depends on the opener plugin's JS permission grant.
#[tauri::command]
pub fn open_notes_folder() -> Result<(), String> {
    let dir = notes_dir().ok_or_else(|| "无法定位笔记目录".to_string())?;
    let path = dir.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}


fn row_to_note(row: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        tags: row.get(3)?,
        source: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

const COLS: &str = "id, title, content, tags, source, created_at, updated_at";

/// 把所有笔记重新镜像到当前的笔记目录。
///
/// 正本在 SQLite，磁盘上的 .md 只是镜像，所以换目录不需要「搬迁」——重新写一遍
/// 就够了。旧目录里的文件留在原地不删：那是用户自己的文件夹，OMNIX 不替他决定。
pub(crate) fn remirror_all(db: &DbManager) -> Result<usize, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("SELECT {COLS} FROM notes"))
        .map_err(|e| e.to_string())?;
    let notes: Vec<Note> = stmt
        .query_map([], row_to_note)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for note in &notes {
        write_note_file(note);
    }
    Ok(notes.len())
}

/// 从磁盘回读比数据库新的 .md，把外部改动收进来。
///
/// 镜像以前是**单向**的：保存时 SQLite → .md，但没有任何地方从磁盘读回。而
/// 「把笔记接入 Agent」会把这个目录注册成 MCP 文件系统服务，agent 是可以**写**
/// 的——写完 OMNIX 毫不知情，界面上还是旧内容，你下次一保存又把它覆盖掉。
///
/// 判定只看 mtime：文件比 `updated_at` 新才导入。相等或更旧一律不动，避免
/// 自己写出去的镜像又被自己读回来。
pub(crate) fn absorb_external_edits(db: &DbManager) -> Result<usize, String> {
    let Some(dir) = notes_dir() else { return Ok(0) };
    let conn = db.get_connection().map_err(|e| e.to_string())?;

    let mut known: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, updated_at FROM notes")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            known.insert(row.0, row.1);
        }
    }

    let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(0) };
    let mut absorbed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        // 目录里可能有用户手放的文件，名字不一定是合法的笔记 id。
        if crate::input_validation::validate_path_component(id, "笔记 id").is_err() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        let file_time = chrono::DateTime::<chrono::Utc>::from(modified);

        if let Some(updated_at) = known.get(id) {
            // `updated_at` 存的是 UTC 的 datetime('now')。
            let db_time = chrono::NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%d %H:%M:%S")
                .map(|naive| naive.and_utc())
                .unwrap_or_else(|_| chrono::DateTime::<chrono::Utc>::MIN_UTC);
            // 留一秒容差：镜像刚写完时两边时间几乎相同，不该判成"外部改动"。
            if file_time <= db_time + chrono::Duration::seconds(1) {
                continue;
            }
        }

        let Ok(raw) = std::fs::read_to_string(&path) else { continue };
        let (title, content) = split_note_file(&raw, id);
        conn.execute(
            "INSERT INTO notes (id, title, content, tags, source)
             VALUES (?1, ?2, ?3, '', 'disk')
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title, content = excluded.content,
                updated_at = datetime('now')",
            params![id, title, content],
        )
        .map_err(|e| e.to_string())?;
        absorbed += 1;
    }
    Ok(absorbed)
}

/// 拆开镜像文件：首行 `# 标题`，空行，其余是正文。不符合这个形状（agent 直接
/// 新建的文件常常没有标题行）就整篇当正文，用文件名当标题。
fn split_note_file(raw: &str, fallback_title: &str) -> (String, String) {
    let mut lines = raw.lines();
    match lines.next() {
        Some(first) if first.starts_with("# ") => {
            let title = first[2..].trim().to_string();
            let body: String = lines.collect::<Vec<_>>().join("\n");
            (title, body.trim_start_matches('\n').trim_end().to_string())
        }
        _ => (fallback_title.to_string(), raw.trim_end().to_string()),
    }
}

#[tauri::command]
pub fn list_notes(query: Option<String>, db: State<'_, Arc<DbManager>>) -> Result<Vec<Note>, String> {
    // 先把磁盘上的外部改动收进来，再列。否则 agent 通过 MCP 写的笔记你永远看不到。
    if let Err(error) = absorb_external_edits(&db) {
        log::warn!("回读笔记目录失败：{error}");
    }
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let term = query.unwrap_or_default();

    // 这里**故意**还是 `LIKE '%词%'`，不是没想过 FTS5。
    //
    // 实测（rusqlite 0.33 bundled SQLite）三个内置分词器对中文的表现：
    //   unicode61 / porter unicode61 —— 一整段中文被当成**一个** token，
    //     「量子计算」搜不到「量子计算的进展」，两字词也搜不到。等于不可用。
    //   trigram —— 三字以上能子串命中，但**两字词一律落空**（苹果 / 会议 /
    //     项目 …），而中文里两字词最常见。
    // 换过去会把「能搜到但排序差」变成「常见词直接搜不到」——是回退，不是优化。
    // 想要真正的中文相关度排序，只能走向量检索（见 knowledge.rs 的 embedding
    // 那半边），那是另一件事：需要每条笔记落一份向量，搜索时也要嵌入一次。
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM notes
             WHERE (?1 = '' OR title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%' OR tags LIKE '%' || ?1 || '%')
             ORDER BY updated_at DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![term], row_to_note)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_note(
    id: Option<String>,
    title: String,
    content: String,
    tags: Option<String>,
    source: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Note, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let id = id.unwrap_or_else(|| format!("note_{}", chrono::Utc::now().timestamp_micros()));
    // id is mirrored to ~/.omnix/notes/<id>.md — reject separators/`..` so a
    // caller-supplied id can't write outside the notes directory.
    crate::input_validation::validate_path_component(&id, "笔记 id")?;
    let title = if title.trim().is_empty() { "无标题笔记".to_string() } else { title };
    conn.execute(
        "INSERT INTO notes (id, title, content, tags, source)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title, content = excluded.content,
            tags = excluded.tags, updated_at = datetime('now')",
        params![id, title.trim(), content, tags.unwrap_or_default(), source.unwrap_or_default()],
    )
    .map_err(|e| e.to_string())?;
    let note = conn
        .query_row(&format!("SELECT {COLS} FROM notes WHERE id = ?1"), params![id], row_to_note)
        .map_err(|e| e.to_string())?;
    // Mirror to ~/.omnix/notes/<id>.md so the note exists as a real file.
    write_note_file(&note);
    Ok(note)
}

#[tauri::command]
pub fn delete_note(id: String, db: State<'_, Arc<DbManager>>) -> Result<(), String> {
    // Mirrored to ~/.omnix/notes/<id>.md and removed via remove_file — same
    // traversal guard as save_note.
    crate::input_validation::validate_path_component(&id, "笔记 id")?;
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    delete_note_file(&id);
    Ok(())
}

#[cfg(test)]
mod note_sync_tests {
    use super::*;

    #[test]
    fn a_file_written_by_an_agent_is_parsed_into_title_and_body() {
        let (title, body) = split_note_file("# 周会纪要\n\n第一条\n第二条\n", "note_1");
        assert_eq!(title, "周会纪要");
        assert_eq!(body, "第一条\n第二条");
    }

    /// agent 直接新建的文件常常没有 `# 标题` 首行——整篇当正文，不要把第一句话
    /// 吃掉当标题。
    #[test]
    fn a_file_without_a_heading_keeps_all_its_text() {
        let (title, body) = split_note_file("就是一段没有标题的文字\n第二行", "note_2");
        assert_eq!(title, "note_2");
        assert_eq!(body, "就是一段没有标题的文字\n第二行");
    }

    /// 往返：写出去再读回来，标题和正文都不能变形。这是双向同步的最低要求——
    /// 否则每次同步都会悄悄改写用户的笔记。
    #[test]
    fn mirroring_round_trips_without_drifting() {
        let note = Note {
            id: "note_rt".into(),
            title: "标题里有 # 井号".into(),
            content: "正文\n\n还有空行\n结尾".into(),
            tags: String::new(),
            source: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let mirrored = format!("# {}\n\n{}\n", note.title, note.content);
        let (title, body) = split_note_file(&mirrored, &note.id);
        assert_eq!(title, note.title);
        assert_eq!(body, note.content, "往返一趟内容就变了，同步会不断改写笔记");
    }
}

