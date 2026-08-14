use super::*;
use crate::db::DbManager;
use crate::input_validation;
use crate::skill_frontmatter::{generate_with_frontmatter, parse_frontmatter, SkillFrontmatter};
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_all_skills(db: State<'_, Arc<DbManager>>) -> Result<Vec<Skill>, String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT name, description, file_path, profile, is_active, dependencies, updated_at, \
         COALESCE(source_type,'local'), source_ref, source_revision, \
         COALESCE(central_path,''), content_hash, starred, category \
         FROM skills",
        )
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let rows = stmt
        .query_map([], |row: &rusqlite::Row| {
            let name: String = row.get(0)?;
            let description: String = row.get(1)?;
            let file_path: String = row.get(2)?;
            let profile: String = row.get(3)?;
            let is_active_int: i32 = row.get(4)?;
            let dependencies_str: String = row.get(5)?;
            let updated_at: String = row.get(6)?;
            let source_type: String = row.get(7)?;
            let source_ref: Option<String> = row.get(8)?;
            let source_revision: Option<String> = row.get(9)?;
            let central_path: String = row.get(10)?;
            let content_hash: Option<String> = row.get(11)?;
            let starred_int: i32 = row.get(12)?;
            let category: Option<String> = row.get(13)?;

            let dependencies: Vec<String> =
                serde_json::from_str(&dependencies_str).unwrap_or_default();

            Ok(Skill {
                name,
                description,
                file_path,
                profile,
                is_active: is_active_int != 0,
                dependencies,
                updated_at,
                source_type,
                source_ref,
                source_revision,
                central_path,
                content_hash,
                starred: starred_int != 0,
                category,
            })
        })
        .map_err(|e: rusqlite::Error| e.to_string())?;

    let mut result = Vec::new();
    for r in rows {
        if let Ok(skill) = r {
            result.push(skill);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn get_skill_content(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<String, String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt
        .query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;

    let mut path = PathBuf::from(&file_path_str);

    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));

    if !path.exists() {
        return Err(format!(
            "Profile file not found at: {}",
            path.to_string_lossy()
        ));
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read skill content: {}", e))?;

    // Parse frontmatter and return body only (frontend gets clean content)
    let (_fm, body) = parse_frontmatter(&raw);
    Ok(body)
}

#[tauri::command]
pub fn save_skill_content(
    name: String,
    profile: String,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT file_path FROM skills WHERE name = ?1")
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let file_path_str: String = stmt
        .query_row(params![name], |r: &rusqlite::Row| r.get(0))
        .map_err(|e: rusqlite::Error| format!("Skill not found: {}", e))?;

    let mut path = PathBuf::from(&file_path_str);

    let suffix = match profile.to_lowercase().as_str() {
        "minimal" => "minimal",
        "comprehensive" => "comprehensive",
        _ => "core",
    };
    path.set_file_name(format!("{}_{}.md", name, suffix));

    let mut tmp_path = path.clone();
    tmp_path.set_extension("tmp");

    std::fs::write(&tmp_path, &content)
        .map_err(|e| format!("Failed to write temporary file: {}", e))?;

    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to atomically replace skill file: {}", e))?;

    conn.execute(
        "UPDATE skills SET updated_at = CURRENT_TIMESTAMP WHERE name = ?1",
        params![name],
    )
    .map_err(|e: rusqlite::Error| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn toggle_skill_active(
    name: String,
    is_active: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET is_active = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![if is_active { 1 } else { 0 }, name],
    )
    .map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_skill_profile(
    name: String,
    profile: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    conn.execute(
        "UPDATE skills SET profile = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![profile, name],
    )
    .map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(())
}

pub(crate) fn create_skill_core(
    db: &DbManager,
    name: &str,
    description: &str,
    profile: &str,
    dependencies: &[String],
    content: &str,
    overwrite: bool,
) -> Result<(), String> {
    // 技能名会直接 `skills_dir().join(name)` 落成目录，所以必须按**路径段**校验：
    // `validate_name` 只查空/长度/控制字符，不拒 `/`、`\`、`..`——`..\..\.ssh`
    // 这种名字会写到技能库外面去，再 sync 一次还会拼进 `~/.claude/skills/`。
    // 同仓库的 zip 导入（skill_pool）和同步（skill_sync）早就用的是这个，
    // 只有创建这一处没跟上。
    input_validation::validate_path_component(name, "name")?;
    let conn = db
        .get_connection()
        .map_err(|e: rusqlite::Error| e.to_string())?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM skills WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists > 0 && !overwrite {
        return Err(format!("技能 {name} 已存在"));
    }

    let base_path = crate::storage::skills_dir().join(name);
    std::fs::create_dir_all(&base_path).map_err(|e| e.to_string())?;
    let base_path_str = base_path.to_string_lossy().to_string();
    let frontmatter = SkillFrontmatter {
        name: Some(name.to_string()),
        description: Some(description.to_string()),
        category: Some("Custom".into()),
        version: Some("1.0.0".into()),
        skills: if dependencies.is_empty() {
            None
        } else {
            Some(dependencies.to_vec())
        },
        ..Default::default()
    };
    let content_with_frontmatter = generate_with_frontmatter(&frontmatter, content);
    for file_name in [
        "SKILL.md".to_string(),
        format!("{name}_core.md"),
        format!("{name}_minimal.md"),
        format!("{name}_comprehensive.md"),
    ] {
        let target = base_path.join(file_name);
        let temporary = target.with_extension("tmp");
        std::fs::write(&temporary, &content_with_frontmatter).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
    }
    let dependencies_json = serde_json::to_string(dependencies).map_err(|e| e.to_string())?;
    let content_hash = crate::hash::fnv1a_hash(&content_with_frontmatter);
    conn.execute(
        "INSERT OR REPLACE INTO skills
         (name, description, file_path, profile, is_active, dependencies, source_type,
          central_path, content_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, 'local', ?3, ?6, CURRENT_TIMESTAMP)",
        params![
            name,
            description,
            base_path_str,
            profile,
            dependencies_json,
            content_hash
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn create_skill(
    name: String,
    description: String,
    profile: String,
    dependencies: Vec<String>,
    content: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    create_skill_core(
        &db,
        &name,
        &description,
        &profile,
        &dependencies,
        &content,
        false,
    )
}

#[cfg(test)]
mod skill_name_path_tests {
    use crate::db::DbManager;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_skillname_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    /// 技能名会直接落成 `skills_dir()/<name>` 目录，所以带路径分隔符的名字
    /// 必须在**写盘之前**被拒。
    ///
    /// 这里以前用的是 `validate_name`，它只查空/长度/控制字符——`..\..\.ssh`
    /// 一路通过，`create_dir_all` 就把目录建到技能库外面了，再 sync 一次还会
    /// 拼进 `~/.claude/skills/`。同仓库的 zip 导入和同步早就用
    /// `validate_path_component` 了，只有创建这一处没跟上。
    #[test]
    fn create_skill_rejects_names_that_escape_the_skills_dir() {
        let db = test_db("escape");
        for evil in [
            "../evil",
            r"..\evil",
            "sub/dir",
            r"sub\dir",
            "..",
            "C:evil",
        ] {
            let result = super::create_skill_core(&db, evil, "d", "p", &[], "c", false);
            assert!(
                result.is_err(),
                "技能名 {evil:?} 能跳出技能目录，却被放行了"
            );
        }
    }

    /// 正常名字不能被误伤——带连字符和中文的技能名是常见写法。
    #[test]
    fn create_skill_still_accepts_ordinary_names() {
        let db = test_db("ok");
        for good in ["gstack-merged", "代码审查", "my_skill.v2"] {
            super::create_skill_core(&db, good, "d", "p", &[], "c", true)
                .unwrap_or_else(|e| panic!("正常技能名 {good:?} 被误拒：{e}"));
        }
    }
}
