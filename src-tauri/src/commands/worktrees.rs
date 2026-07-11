//! Parallel sessions via Git worktrees.
//!
//! Running several agents against one repository at the same time is unsafe if
//! they share a single working tree — their edits collide. `git worktree` gives
//! each session its own checked-out tree on its own branch, all backed by the
//! one `.git` object store. OMNIX creates an isolated worktree per session under
//! a sibling `.omnix-worktrees/` directory, tracks the mapping in SQLite, and
//! lets the user inspect / merge / remove each one.
//!
//! R3 scope: Git repositories only. Merge surfaces conflicts honestly (it does
//! not auto-resolve) and never force-pushes or rewrites history.

use std::path::{Path, PathBuf};
use crate::proc::NoWindow;
use std::process::Command;
use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize)]
pub struct Worktree {
    pub id: String,
    pub repo_path: String,
    pub worktree_path: String,
    pub branch: String,
    pub session_id: String,
    pub label: String,
    pub created_at: String,
    pub is_main: bool,
    pub exists: bool,
    pub dirty: bool,
    pub ahead: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeResult {
    pub merged: bool,
    pub conflict: bool,
    pub message: String,
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .no_window()
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("git 执行失败: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Strip the Windows `\\?\` verbatim-path prefix that `canonicalize()` adds.
/// Git's `worktree add` cannot create leading directories under such a path.
fn git_path(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    value
        .strip_prefix(r"\\?\")
        .map(|stripped| stripped.to_string())
        .unwrap_or(value)
}

fn is_git_repo(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"])
        .map(|value| value == "true")
        .unwrap_or(false)
}

/// Resolve to the repository's top-level working directory so every worktree is
/// recorded against one canonical key regardless of which subdir was passed in.
fn repo_top_level(workspace_path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(workspace_path.trim())
        .canonicalize()
        .map_err(|error| format!("工作区不存在或无法访问: {error}"))?;
    if !root.is_dir() {
        return Err("工作区路径不是文件夹".into());
    }
    if !is_git_repo(&root) {
        return Err("当前工作区不是 Git 仓库，无法创建并行 worktree".into());
    }
    let top = git(&root, &["rev-parse", "--show-toplevel"])?;
    PathBuf::from(&top)
        .canonicalize()
        .map_err(|error| format!("无法定位仓库根目录: {error}"))
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS worktrees (
            id TEXT PRIMARY KEY,
            repo_path TEXT NOT NULL,
            worktree_path TEXT NOT NULL,
            branch TEXT NOT NULL DEFAULT '',
            session_id TEXT NOT NULL DEFAULT '',
            label TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn sanitize_segment(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() { "session".into() } else { trimmed }
}

fn worktree_is_dirty(path: &Path) -> bool {
    git(path, &["status", "--porcelain"])
        .map(|out| !out.is_empty())
        .unwrap_or(false)
}

/// Commits on `branch` that are not yet in the main repo's current HEAD.
fn commits_ahead(repo: &Path, branch: &str) -> u32 {
    let head = match git(repo, &["rev-parse", "HEAD"]) {
        Ok(value) => value,
        Err(_) => return 0,
    };
    git(repo, &["rev-list", "--count", &format!("{head}..{branch}")])
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

pub fn create_worktree_core(
    db: &DbManager,
    workspace_path: &str,
    session_id: &str,
    label: &str,
    branch: Option<&str>,
) -> Result<Worktree, String> {
    ensure_table(db)?;
    let repo = repo_top_level(workspace_path)?;

    let id = format!("wt_{}", chrono::Utc::now().timestamp_micros());
    let short = &id[id.len().saturating_sub(6)..];
    let repo_name = repo
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into());

    // Branch name: caller-supplied (sanitized) or derived from the session.
    let branch_seed = branch
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{}-{}", sanitize_segment(session_id), short));
    let branch_name = format!("omnix/{}", sanitize_segment(&branch_seed));

    // Place worktrees as a sibling directory so they never sit inside the repo's
    // own working tree (which confuses status/ignore handling).
    let parent = repo.parent().ok_or("无法定位仓库父目录")?;
    let trees_dir = parent.join(".omnix-worktrees");
    std::fs::create_dir_all(&trees_dir)
        .map_err(|error| format!("无法创建 worktree 目录: {error}"))?;
    let worktree_path = trees_dir.join(format!("{repo_name}__{}", sanitize_segment(&branch_seed)));
    if worktree_path.exists() {
        return Err(format!("目标 worktree 已存在: {}", worktree_path.display()));
    }
    let worktree_str = git_path(&worktree_path);

    // Create a fresh branch at the current HEAD and check it out in the new tree.
    let branch_exists = git(&repo, &["rev-parse", "--verify", "--quiet", &branch_name]).is_ok();
    if branch_exists {
        git(&repo, &["worktree", "add", &worktree_str, &branch_name])?;
    } else {
        git(&repo, &["worktree", "add", "-b", &branch_name, &worktree_str, "HEAD"])?;
    }

    let created_at = chrono::Utc::now().to_rfc3339();
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO worktrees (id, repo_path, worktree_path, branch, session_id, label, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            git_path(&repo),
            worktree_str,
            branch_name,
            session_id,
            label,
            created_at
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(Worktree {
        id,
        repo_path: git_path(&repo),
        worktree_path: worktree_str,
        branch: branch_name,
        session_id: session_id.to_string(),
        label: label.to_string(),
        created_at,
        is_main: false,
        exists: true,
        dirty: false,
        ahead: 0,
    })
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_worktree(
    workspace_path: String,
    session_id: String,
    label: String,
    branch: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Worktree, String> {
    create_worktree_core(&db, &workspace_path, &session_id, &label, branch.as_deref())
}

#[tauri::command]
pub fn list_worktrees(
    workspace_path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Worktree>, String> {
    ensure_table(&db)?;
    let repo = repo_top_level(&workspace_path)?;

    // DB metadata (session/label) keyed by worktree path.
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare("SELECT id, worktree_path, branch, session_id, label, created_at FROM worktrees WHERE repo_path = ?1")
        .map_err(|error| error.to_string())?;
    let recorded: std::collections::HashMap<String, (String, String, String, String, String)> = statement
        .query_map(params![git_path(&repo)], |row| {
            Ok((
                row.get::<_, String>(1)?, // worktree_path (key)
                (
                    row.get::<_, String>(0)?, // id
                    row.get::<_, String>(2)?, // branch
                    row.get::<_, String>(3)?, // session_id
                    row.get::<_, String>(4)?, // label
                    row.get::<_, String>(5)?, // created_at
                ),
            ))
        })
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|(path, meta)| (path.replace('\\', "/"), meta))
        .collect();

    // Ground truth from git: every live worktree, including the main one.
    let porcelain = git(&repo, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();
    let mut cur_path: Option<String> = None;
    let mut cur_branch = String::new();
    let mut is_main = true; // git lists the main worktree first

    let mut flush = |path: Option<String>, branch: &str, is_main: bool, out: &mut Vec<Worktree>| {
        let Some(path) = path else { return };
        let path_buf = PathBuf::from(&path);
        let key = path.replace('\\', "/");
        let meta = recorded.get(&key);
        let exists = path_buf.exists();
        let branch_short = branch.strip_prefix("refs/heads/").unwrap_or(branch).to_string();
        out.push(Worktree {
            id: meta.map(|m| m.0.clone()).unwrap_or_default(),
            repo_path: git_path(&repo),
            worktree_path: path,
            branch: if branch_short.is_empty() { meta.map(|m| m.1.clone()).unwrap_or_default() } else { branch_short.clone() },
            session_id: meta.map(|m| m.2.clone()).unwrap_or_default(),
            label: meta.map(|m| m.3.clone()).unwrap_or_default(),
            created_at: meta.map(|m| m.4.clone()).unwrap_or_default(),
            is_main,
            exists,
            dirty: exists && worktree_is_dirty(&path_buf),
            ahead: if is_main || branch_short.is_empty() { 0 } else { commits_ahead(&repo, &branch_short) },
        });
    };

    for line in porcelain.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            // Starting a new record — flush the previous one.
            flush(cur_path.take(), &cur_branch, is_main, &mut worktrees);
            if !worktrees.is_empty() {
                is_main = false;
            }
            cur_path = Some(path.to_string());
            cur_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            cur_branch = branch.to_string();
        }
    }
    flush(cur_path.take(), &cur_branch, is_main, &mut worktrees);

    Ok(worktrees)
}

#[tauri::command]
pub fn remove_worktree(
    worktree_id: String,
    delete_branch: bool,
    force: bool,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let (repo_path, worktree_path, branch): (String, String, String) = conn
        .query_row(
            "SELECT repo_path, worktree_path, branch FROM worktrees WHERE id = ?1",
            params![worktree_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| format!("找不到 worktree {worktree_id}: {error}"))?;
    let repo = PathBuf::from(&repo_path);

    let mut args = vec!["worktree", "remove", &worktree_path];
    if force {
        args.push("--force");
    }
    // Ignore "already gone" errors so a manually-deleted dir can still be pruned.
    if let Err(error) = git(&repo, &args) {
        if !error.contains("is not a working tree") && !error.contains("No such file") {
            let _ = git(&repo, &["worktree", "prune"]);
            if !force {
                return Err(error);
            }
        }
    }
    let _ = git(&repo, &["worktree", "prune"]);

    if delete_branch && !branch.is_empty() {
        let flag = if force { "-D" } else { "-d" };
        let _ = git(&repo, &["branch", flag, &branch]);
    }

    conn.execute("DELETE FROM worktrees WHERE id = ?1", params![worktree_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Merge a worktree's branch into the main repo's current branch. Surfaces
/// conflicts instead of resolving them; on conflict the merge is aborted so the
/// main tree is left clean.
#[tauri::command]
pub fn merge_worktree(
    worktree_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<MergeResult, String> {
    ensure_table(&db)?;
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let (repo_path, branch): (String, String) = conn
        .query_row(
            "SELECT repo_path, branch FROM worktrees WHERE id = ?1",
            params![worktree_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| format!("找不到 worktree {worktree_id}: {error}"))?;
    let repo = PathBuf::from(&repo_path);

    if branch.is_empty() {
        return Err("该 worktree 没有关联分支".into());
    }
    // The main tree must be clean to accept a merge safely.
    if worktree_is_dirty(&repo) {
        return Err("主工作区有未提交改动，请先提交或暂存后再合并".into());
    }

    let target = git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "HEAD".into());
    match git(
        &repo,
        &["merge", "--no-ff", "-m", &format!("omnix: merge worktree {branch}"), &branch],
    ) {
        Ok(message) => Ok(MergeResult {
            merged: true,
            conflict: false,
            message: format!("已合并 {branch} → {target}\n{message}"),
        }),
        Err(error) => {
            // Leave the main tree clean: abort the half-applied merge.
            let _ = git(&repo, &["merge", "--abort"]);
            let conflict = error.contains("conflict") || error.contains("CONFLICT");
            Ok(MergeResult {
                merged: false,
                conflict,
                message: if conflict {
                    format!("合并 {branch} 时存在冲突，已自动取消合并。请手动处理：\n{error}")
                } else {
                    format!("合并失败：{error}")
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {:?} failed", args);
    }

    #[test]
    fn create_list_merge_remove_round_trip() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let base = std::env::temp_dir().join(format!(
            "omnix_wt_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let root = base.join("repo");
        std::fs::create_dir_all(&root).expect("root");
        run(&root, &["init", "-q"]);
        run(&root, &["config", "user.email", "t@t"]);
        run(&root, &["config", "user.name", "t"]);
        std::fs::write(root.join("a.txt"), "base\n").expect("a");
        run(&root, &["add", "-A"]);
        run(&root, &["commit", "-qm", "init"]);

        let db_path = std::env::temp_dir().join(format!(
            "omnix_wt_db_{}.sqlite",
            chrono::Utc::now().timestamp_micros()
        ));
        let db = DbManager::new_runtime_test(db_path.clone());

        // Create an isolated worktree.
        let wt = create_worktree_core(&db, root.to_string_lossy().as_ref(), "s1", "feature A", None)
            .expect("create worktree");
        assert!(PathBuf::from(&wt.worktree_path).exists());
        assert!(wt.branch.starts_with("omnix/"));

        // List shows the main tree + the new one.
        let listed = list_worktrees_for_test(&db, root.to_string_lossy().as_ref()).expect("list");
        assert!(listed.iter().any(|w| w.is_main));
        assert!(listed.iter().any(|w| w.id == wt.id && !w.is_main));

        // Commit work inside the worktree, then it should be ahead.
        let wt_path = PathBuf::from(&wt.worktree_path);
        std::fs::write(wt_path.join("b.txt"), "from worktree\n").expect("b");
        run(&wt_path, &["add", "-A"]);
        run(&wt_path, &["commit", "-qm", "work"]);
        let listed = list_worktrees_for_test(&db, root.to_string_lossy().as_ref()).expect("list2");
        let mine = listed.iter().find(|w| w.id == wt.id).unwrap();
        assert_eq!(mine.ahead, 1);

        // Merge clean back into main, then b.txt appears in the main tree.
        let result = merge_for_test(&db, &wt.id).expect("merge");
        assert!(result.merged, "merge message: {}", result.message);
        assert!(root.join("b.txt").exists());

        // Remove cleans up the worktree dir.
        remove_for_test(&db, &wt.id).expect("remove");
        assert!(!wt_path.exists());

        drop(db);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_dir_all(base);
    }

    // Helpers mirror the command bodies without a Tauri `State`.
    fn list_worktrees_for_test(db: &DbManager, ws: &str) -> Result<Vec<Worktree>, String> {
        ensure_table(db)?;
        let repo = repo_top_level(ws)?;
        let porcelain = git(&repo, &["worktree", "list", "--porcelain"])?;
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, worktree_path, branch FROM worktrees WHERE repo_path = ?1")
            .map_err(|e| e.to_string())?;
        let recorded: std::collections::HashMap<String, (String, String)> = stmt
            .query_map(params![git_path(&repo)], |row| {
                Ok((row.get::<_, String>(1)?.replace('\\', "/"), (row.get::<_, String>(0)?, row.get::<_, String>(2)?)))
            })
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        let mut out = Vec::new();
        let mut cur: Option<String> = None;
        let mut branch = String::new();
        let mut is_main = true;
        for line in porcelain.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                if let Some(path) = cur.take() {
                    push_test(&repo, &recorded, path, &branch, is_main, &mut out);
                    is_main = false;
                }
                cur = Some(p.to_string());
                branch = String::new();
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = b.to_string();
            }
        }
        if let Some(path) = cur.take() {
            push_test(&repo, &recorded, path, &branch, is_main, &mut out);
        }
        Ok(out)
    }

    fn push_test(
        repo: &Path,
        recorded: &std::collections::HashMap<String, (String, String)>,
        path: String,
        branch: &str,
        is_main: bool,
        out: &mut Vec<Worktree>,
    ) {
        let key = path.replace('\\', "/");
        let meta = recorded.get(&key);
        let branch_short = branch.strip_prefix("refs/heads/").unwrap_or(branch).to_string();
        out.push(Worktree {
            id: meta.map(|m| m.0.clone()).unwrap_or_default(),
            repo_path: git_path(repo),
            worktree_path: path.clone(),
            branch: branch_short.clone(),
            session_id: String::new(),
            label: String::new(),
            created_at: String::new(),
            is_main,
            exists: PathBuf::from(&path).exists(),
            dirty: false,
            ahead: if is_main || branch_short.is_empty() { 0 } else { commits_ahead(repo, &branch_short) },
        });
    }

    fn merge_for_test(db: &DbManager, id: &str) -> Result<MergeResult, String> {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let (repo_path, branch): (String, String) = conn
            .query_row("SELECT repo_path, branch FROM worktrees WHERE id = ?1", params![id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| e.to_string())?;
        let repo = PathBuf::from(&repo_path);
        match git(&repo, &["merge", "--no-ff", "-m", "merge", &branch]) {
            Ok(m) => Ok(MergeResult { merged: true, conflict: false, message: m }),
            Err(e) => {
                let _ = git(&repo, &["merge", "--abort"]);
                Ok(MergeResult { merged: false, conflict: e.contains("CONFLICT"), message: e })
            }
        }
    }

    fn remove_for_test(db: &DbManager, id: &str) -> Result<(), String> {
        let conn = db.get_connection().map_err(|e| e.to_string())?;
        let (repo_path, worktree_path): (String, String) = conn
            .query_row("SELECT repo_path, worktree_path FROM worktrees WHERE id = ?1", params![id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| e.to_string())?;
        let repo = PathBuf::from(&repo_path);
        git(&repo, &["worktree", "remove", &worktree_path, "--force"])?;
        conn.execute("DELETE FROM worktrees WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
        Ok(())
    }
}
