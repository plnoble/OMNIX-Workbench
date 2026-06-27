//! Workspace checkpoints + per-file diff review (Claude Code / Codex desktop
//! inspired). Before an agent turn that may modify files, a checkpoint snapshots
//! the whole working tree onto a **shadow ref** (`refs/omnix/checkpoints/<id>`)
//! using a temporary index — so it captures tracked AND untracked (non-ignored)
//! content without touching the user's real index, working tree, or `git log`.
//!
//! The user can review the agent's changes as per-file diffs (accept = keep,
//! reject = restore that file from the checkpoint) and rewind the whole
//! workspace to any checkpoint (a pre-restore checkpoint is made first so the
//! rewind itself is undoable).
//!
//! R1 scope: Git workspaces. Non-Git workspaces report a skipped checkpoint
//! (callers treat it as a no-op) rather than failing the turn.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize)]
pub struct Checkpoint {
    pub id: String,
    pub workspace_path: String,
    pub session_id: String,
    pub label: String,
    pub vcs: String,
    pub ref_name: String,
    pub created_at: String,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub status: String, // A | M | D | R | ...
    pub additions: u32,
    pub deletions: u32,
    pub unified_diff: String,
}

fn git(root: &Path, args: &[&str], index: Option<&Path>, envs: &[(&str, &str)]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(args);
    if let Some(index_path) = index {
        command.env("GIT_INDEX_FILE", index_path);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().map_err(|error| format!("git 执行失败: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn is_git_repo(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"], None, &[])
        .map(|value| value == "true")
        .unwrap_or(false)
}

fn temp_index() -> PathBuf {
    std::env::temp_dir().join(format!(
        "omnix-cp-index-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

const IDENTITY: &[(&str, &str)] = &[
    ("GIT_AUTHOR_NAME", "OMNIX Workbench"),
    ("GIT_AUTHOR_EMAIL", "checkpoint@omnix.local"),
    ("GIT_COMMITTER_NAME", "OMNIX Workbench"),
    ("GIT_COMMITTER_EMAIL", "checkpoint@omnix.local"),
];

fn canonical_root(workspace_path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(workspace_path.trim())
        .canonicalize()
        .map_err(|error| format!("工作区不存在或无法访问: {error}"))?;
    if !root.is_dir() {
        return Err("工作区路径不是文件夹".into());
    }
    Ok(root)
}

fn ensure_table(db: &DbManager) -> Result<(), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            id TEXT PRIMARY KEY,
            workspace_path TEXT NOT NULL,
            session_id TEXT NOT NULL DEFAULT '',
            label TEXT NOT NULL DEFAULT '',
            vcs TEXT NOT NULL DEFAULT 'git',
            ref_name TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn lookup_checkpoint(db: &DbManager, id: &str) -> Result<(String, String), String> {
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT workspace_path, ref_name FROM checkpoints WHERE id = ?1",
        params![id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .map_err(|error| format!("找不到检查点 {id}: {error}"))
}

/// Snapshot the whole working tree onto a shadow ref. Returns a `skipped`
/// checkpoint for non-Git workspaces so the caller can ignore it silently.
pub fn create_checkpoint_core(
    db: &DbManager,
    workspace_path: &str,
    session_id: &str,
    label: &str,
) -> Result<Checkpoint, String> {
    ensure_table(db)?;
    let root = canonical_root(workspace_path)?;
    let id = format!("cp_{}", chrono::Utc::now().timestamp_micros());

    if !is_git_repo(&root) {
        return Ok(Checkpoint {
            id,
            workspace_path: root.to_string_lossy().into_owned(),
            session_id: session_id.to_string(),
            label: label.to_string(),
            vcs: "none".into(),
            ref_name: String::new(),
            created_at: String::new(),
            skipped: true,
        });
    }

    let index = temp_index();
    let _ = std::fs::remove_file(&index);
    // Stage every non-ignored file (tracked + untracked) into a throwaway index.
    git(&root, &["add", "-A"], Some(&index), &[])?;
    let tree = git(&root, &["write-tree"], Some(&index), &[])?;
    let _ = std::fs::remove_file(&index);

    let message = format!("omnix-checkpoint: {label}");
    let parent = git(&root, &["rev-parse", "HEAD"], None, &[]).ok();
    let commit = match parent {
        Some(parent) => git(
            &root,
            &["commit-tree", &tree, "-p", &parent, "-m", &message],
            None,
            IDENTITY,
        )?,
        None => git(&root, &["commit-tree", &tree, "-m", &message], None, IDENTITY)?,
    };
    let ref_name = format!("refs/omnix/checkpoints/{id}");
    git(&root, &["update-ref", &ref_name, &commit], None, &[])?;

    let created_at = chrono::Utc::now().to_rfc3339();
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO checkpoints (id, workspace_path, session_id, label, vcs, ref_name, created_at)
         VALUES (?1, ?2, ?3, ?4, 'git', ?5, ?6)",
        params![id, root.to_string_lossy(), session_id, label, ref_name, created_at],
    )
    .map_err(|error| error.to_string())?;

    Ok(Checkpoint {
        id,
        workspace_path: root.to_string_lossy().into_owned(),
        session_id: session_id.to_string(),
        label: label.to_string(),
        vcs: "git".into(),
        ref_name,
        created_at,
        skipped: false,
    })
}

/// Compute per-file diffs between a base (a checkpoint ref, or HEAD) and the
/// current working tree, using a temp index so untracked files are included.
fn diff_against(root: &Path, base: &str) -> Result<Vec<FileDiff>, String> {
    let index = temp_index();
    let _ = std::fs::remove_file(&index);
    git(root, &["add", "-A"], Some(&index), &[])?;

    let name_status = git(root, &["diff", "--cached", "--name-status", base], Some(&index), &[])?;
    let numstat = git(root, &["diff", "--cached", "--numstat", base], Some(&index), &[])?;

    // path -> (additions, deletions)
    let mut counts: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    for line in numstat.lines() {
        let mut parts = line.split('\t');
        let adds = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        let dels = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        if let Some(path) = parts.next() {
            counts.insert(path.to_string(), (adds, dels));
        }
    }

    let mut diffs = Vec::new();
    for line in name_status.lines() {
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or("").chars().next().unwrap_or('?').to_string();
        let path = match parts.last() {
            Some(path) if !path.is_empty() => path.to_string(),
            _ => continue,
        };
        let unified = git(root, &["diff", "--cached", base, "--", &path], Some(&index), &[])
            .unwrap_or_default();
        let (additions, deletions) = counts.get(&path).copied().unwrap_or((0, 0));
        diffs.push(FileDiff {
            path,
            status,
            additions,
            deletions,
            unified_diff: unified,
        });
    }
    let _ = std::fs::remove_file(&index);
    Ok(diffs)
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_checkpoint(
    workspace_path: String,
    session_id: String,
    label: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Checkpoint, String> {
    create_checkpoint_core(&db, &workspace_path, &session_id, &label)
}

#[tauri::command]
pub fn list_checkpoints(
    workspace_path: String,
    session_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<Checkpoint>, String> {
    ensure_table(&db)?;
    let root = canonical_root(&workspace_path)?;
    let conn = db.get_connection().map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT id, workspace_path, session_id, label, vcs, ref_name, created_at
             FROM checkpoints
             WHERE workspace_path = ?1 AND (?2 IS NULL OR session_id = ?2)
             ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![root.to_string_lossy(), session_id], |row| {
            Ok(Checkpoint {
                id: row.get(0)?,
                workspace_path: row.get(1)?,
                session_id: row.get(2)?,
                label: row.get(3)?,
                vcs: row.get(4)?,
                ref_name: row.get(5)?,
                created_at: row.get(6)?,
                skipped: false,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_workspace_diff(
    workspace_path: String,
    checkpoint_id: Option<String>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<FileDiff>, String> {
    let root = canonical_root(&workspace_path)?;
    if !is_git_repo(&root) {
        return Err("逐文件 diff 目前仅支持 Git 工作区".into());
    }
    let base = match checkpoint_id {
        Some(id) => lookup_checkpoint(&db, &id)?.1,
        None => "HEAD".to_string(),
    };
    diff_against(&root, &base)
}

#[tauri::command]
pub fn restore_checkpoint(
    checkpoint_id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<Checkpoint, String> {
    let (workspace_path, ref_name) = lookup_checkpoint(&db, &checkpoint_id)?;
    let root = canonical_root(&workspace_path)?;

    // Make the rewind itself undoable.
    let _ = create_checkpoint_core(&db, &workspace_path, "", "回退前自动备份");

    // Write every file from the checkpoint tree back to the working tree.
    let index = temp_index();
    let _ = std::fs::remove_file(&index);
    git(&root, &["read-tree", &ref_name], Some(&index), &[])?;
    git(&root, &["checkout-index", "-a", "-f"], Some(&index), &[])?;

    // Remove files that exist now but were not in the checkpoint (newly created).
    let checkpoint_files: HashSet<String> = git(&root, &["ls-tree", "-r", "--name-only", &ref_name], None, &[])?
        .lines()
        .map(str::to_string)
        .collect();
    let tracked = git(&root, &["ls-files"], None, &[]).unwrap_or_default();
    let untracked = git(&root, &["ls-files", "--others", "--exclude-standard"], None, &[]).unwrap_or_default();
    for path in tracked.lines().chain(untracked.lines()) {
        if !path.is_empty() && !checkpoint_files.contains(path) {
            let _ = std::fs::remove_file(root.join(path));
        }
    }
    let _ = std::fs::remove_file(&index);

    Ok(Checkpoint {
        id: checkpoint_id,
        workspace_path: root.to_string_lossy().into_owned(),
        session_id: String::new(),
        label: "已回退".into(),
        vcs: "git".into(),
        ref_name,
        created_at: chrono::Utc::now().to_rfc3339(),
        skipped: false,
    })
}

#[tauri::command]
pub fn revert_file(
    checkpoint_id: String,
    path: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    let (workspace_path, ref_name) = lookup_checkpoint(&db, &checkpoint_id)?;
    let root = canonical_root(&workspace_path)?;
    let existed = git(&root, &["cat-file", "-e", &format!("{ref_name}:{path}")], None, &[]).is_ok();
    if existed {
        git(&root, &["restore", "--source", &ref_name, "--worktree", "--", &path], None, &[])?;
    } else {
        // File is new since the checkpoint — reverting means deleting it.
        std::fs::remove_file(root.join(&path)).map_err(|error| format!("无法删除 {path}: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .envs(IDENTITY.iter().copied())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {:?} failed", args);
    }

    #[test]
    fn checkpoint_diff_and_restore_round_trip() {
        // Skip gracefully if git is unavailable in the test environment.
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!("omnix_cp_test_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()));
        std::fs::create_dir_all(&root).expect("root");
        run(&root, &["init", "-q"]);
        std::fs::write(root.join("a.txt"), "original\n").expect("a");
        run(&root, &["add", "-A"]);
        run(&root, &["commit", "-qm", "init"]);

        let db_path = std::env::temp_dir().join(format!("omnix_cp_db_{}.sqlite", chrono::Utc::now().timestamp_micros()));
        let db = DbManager::new_runtime_test(db_path.clone());

        let cp = create_checkpoint_core(&db, root.to_string_lossy().as_ref(), "s1", "before edit").expect("checkpoint");
        assert!(!cp.skipped);

        // Agent edits a.txt and creates a new untracked file.
        std::fs::write(root.join("a.txt"), "changed\n").expect("edit a");
        std::fs::write(root.join("new.txt"), "brand new\n").expect("new");

        let diffs = diff_against(&root, &cp.ref_name).expect("diff");
        let paths: Vec<&str> = diffs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"new.txt"));
        let a = diffs.iter().find(|d| d.path == "a.txt").unwrap();
        assert_eq!(a.status, "M");
        let n = diffs.iter().find(|d| d.path == "new.txt").unwrap();
        assert_eq!(n.status, "A");

        // Restore: a.txt reverts, new.txt is removed.
        restore_for_test(&db, &cp.id).expect("restore");
        // Normalize line endings — git autocrlf may rewrite LF to CRLF on Windows.
        let restored = std::fs::read_to_string(root.join("a.txt")).unwrap().replace("\r\n", "\n");
        assert_eq!(restored, "original\n");
        assert!(!root.join("new.txt").exists());

        drop(db);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Mirrors `restore_checkpoint`'s body without a Tauri `State`.
    fn restore_for_test(db: &DbManager, id: &str) -> Result<(), String> {
        let (workspace_path, ref_name) = lookup_checkpoint(db, id)?;
        let root = canonical_root(&workspace_path)?;
        let index = temp_index();
        let _ = std::fs::remove_file(&index);
        git(&root, &["read-tree", &ref_name], Some(&index), &[])?;
        git(&root, &["checkout-index", "-a", "-f"], Some(&index), &[])?;
        let checkpoint_files: HashSet<String> = git(&root, &["ls-tree", "-r", "--name-only", &ref_name], None, &[])?
            .lines().map(str::to_string).collect();
        let tracked = git(&root, &["ls-files"], None, &[]).unwrap_or_default();
        let untracked = git(&root, &["ls-files", "--others", "--exclude-standard"], None, &[]).unwrap_or_default();
        for p in tracked.lines().chain(untracked.lines()) {
            if !p.is_empty() && !checkpoint_files.contains(p) {
                let _ = std::fs::remove_file(root.join(p));
            }
        }
        let _ = std::fs::remove_file(&index);
        Ok(())
    }
}
