use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceChange {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSnapshot {
    pub root_path: String,
    pub root_name: String,
    pub branch: Option<String>,
    pub changes: Vec<WorkspaceChange>,
    pub files: Vec<WorkspaceFileEntry>,
    pub truncated: bool,
}

const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".cache",
];

fn collect_workspace_files(
    root: &Path,
    max_depth: usize,
    max_entries: usize,
) -> Result<Vec<WorkspaceFileEntry>, String> {
    fn visit(
        root: &Path,
        directory: &Path,
        depth: usize,
        max_depth: usize,
        max_entries: usize,
        entries: &mut Vec<WorkspaceFileEntry>,
    ) -> Result<(), String> {
        if depth > max_depth || entries.len() >= max_entries {
            return Ok(());
        }
        let mut children = std::fs::read_dir(directory)
            .map_err(|error| format!("无法读取 {}: {error}", directory.display()))?
            .flatten()
            .collect::<Vec<_>>();
        children.sort_by(|left, right| {
            let left_dir = left.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            let right_dir = right.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            right_dir.cmp(&left_dir).then_with(|| {
                left.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(&right.file_name().to_string_lossy().to_lowercase())
            })
        });
        for child in children {
            if entries.len() >= max_entries {
                break;
            }
            let file_type = child.file_type().map_err(|error| error.to_string())?;
            if file_type.is_symlink() {
                continue;
            }
            let name = child.file_name().to_string_lossy().into_owned();
            if file_type.is_dir() && IGNORED_DIRECTORIES.contains(&name.as_str()) {
                continue;
            }
            let path = child.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(WorkspaceFileEntry {
                path: relative,
                name,
                is_dir: file_type.is_dir(),
                depth,
            });
            if file_type.is_dir() && depth < max_depth {
                visit(root, &path, depth + 1, max_depth, max_entries, entries)?;
            }
        }
        Ok(())
    }

    let mut entries = Vec::new();
    visit(root, root, 0, max_depth, max_entries, &mut entries)?;
    Ok(entries)
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn workspace_changes(root: &Path) -> Vec<WorkspaceChange> {
    git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            if line.len() < 3 {
                return None;
            }
            Some(WorkspaceChange {
                status: line[..2].trim().to_string(),
                path: line[3..].trim().trim_matches('"').replace(" -> ", " → "),
            })
        })
        .collect()
}

#[tauri::command]
pub fn get_workspace_snapshot(workspace_path: String) -> Result<WorkspaceSnapshot, String> {
    let requested = PathBuf::from(workspace_path.trim());
    let root = requested
        .canonicalize()
        .map_err(|error| format!("工作区不存在或无法访问: {error}"))?;
    if !root.is_dir() {
        return Err("工作区路径不是文件夹".into());
    }
    let max_entries = 600;
    let files = collect_workspace_files(&root, 4, max_entries)?;
    Ok(WorkspaceSnapshot {
        root_path: root.to_string_lossy().into_owned(),
        root_name: root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string_lossy().into_owned()),
        branch: git_output(&root, &["branch", "--show-current"])
            .filter(|branch| !branch.is_empty()),
        changes: workspace_changes(&root),
        truncated: files.len() >= max_entries,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::collect_workspace_files;

    #[test]
    fn workspace_tree_is_relative_and_skips_generated_directories() {
        let root = std::env::temp_dir().join(format!(
            "omnix_workspace_tree_{}",
            chrono::Utc::now().timestamp_micros()
        ));
        std::fs::create_dir_all(root.join("src/components")).expect("source dirs");
        std::fs::create_dir_all(root.join("node_modules/pkg")).expect("generated dirs");
        std::fs::write(root.join("src/main.ts"), "export {};").expect("source file");
        std::fs::write(root.join("src/components/App.tsx"), "export const App = 1;")
            .expect("nested source file");
        std::fs::write(root.join("node_modules/pkg/index.js"), "ignored").expect("generated file");

        let files = collect_workspace_files(&root, 4, 100).expect("workspace tree");
        let serialized = serde_json::to_string(&files).expect("tree JSON");
        assert!(serialized.contains("src/main.ts"));
        assert!(serialized.contains("src/components/App.tsx"));
        assert!(!serialized.contains("node_modules"));
        assert!(!serialized.contains(root.to_string_lossy().as_ref()));

        let _ = std::fs::remove_dir_all(root);
    }
}
