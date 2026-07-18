use std::path::{Path, PathBuf};
use crate::proc::NoWindow;
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
    // Wall-clock budget on the WHOLE walk. Even with node_modules/target ignored
    // and a depth/entry cap, a repo on a slow drive or behind real-time AV
    // (每次 read_dir/file_type 都过火绒) can crawl for many seconds. On budget
    // exhaustion we return what we have — a partial tree beats an infinite spinner.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);

    fn visit(
        root: &Path,
        directory: &Path,
        depth: usize,
        max_depth: usize,
        max_entries: usize,
        deadline: std::time::Instant,
        entries: &mut Vec<WorkspaceFileEntry>,
    ) -> Result<(), String> {
        if depth > max_depth || entries.len() >= max_entries || std::time::Instant::now() >= deadline {
            return Ok(());
        }
        let Ok(read) = std::fs::read_dir(directory) else {
            return Ok(()); // unreadable dir → skip, don't fail the whole snapshot
        };
        let mut children = read.flatten().collect::<Vec<_>>();
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
            if entries.len() >= max_entries || std::time::Instant::now() >= deadline {
                break;
            }
            let Ok(file_type) = child.file_type() else { continue };
            if file_type.is_symlink() {
                continue;
            }
            let name = child.file_name().to_string_lossy().into_owned();
            if file_type.is_dir() && IGNORED_DIRECTORIES.contains(&name.as_str()) {
                continue;
            }
            let path = child.path();
            let relative = match path.strip_prefix(root) {
                Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            entries.push(WorkspaceFileEntry {
                path: relative,
                name,
                is_dir: file_type.is_dir(),
                depth,
            });
            if file_type.is_dir() && depth < max_depth {
                visit(root, &path, depth + 1, max_depth, max_entries, deadline, entries)?;
            }
        }
        Ok(())
    }

    let mut entries = Vec::new();
    visit(root, root, 0, max_depth, max_entries, deadline, &mut entries)?;
    Ok(entries)
}

/// Run git with a hard wall-clock timeout. Without it, `git status` on a repo
/// with a huge untracked tree (or a held index.lock, or a slow network drive)
/// blocks the whole workspace snapshot forever — the "读工作区读了很久都没读好"
/// symptom. On timeout we kill the child and treat it as "no git info".
fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    use std::process::Stdio;
    let mut child = Command::new("git")
        .no_window()
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut buf = String::new();
                use std::io::Read;
                child.stdout.take()?.read_to_string(&mut buf).ok()?;
                return Some(buf.trim().to_string());
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            Err(_) => return None,
        }
    }
}

fn workspace_changes(root: &Path) -> Vec<WorkspaceChange> {
    // `--untracked-files=normal` (not `all`): shows untracked dirs without
    // recursing into every file inside them, so a repo with an unignored
    // node_modules/target doesn't take minutes. Cap the list so a repo with
    // thousands of changes produces a bounded payload.
    git_output(
        root,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )
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
    .take(500)
    .collect()
}

fn build_workspace_snapshot(root: PathBuf) -> WorkspaceSnapshot {
    let max_entries = 600;
    let files = collect_workspace_files(&root, 4, max_entries).unwrap_or_default();
    WorkspaceSnapshot {
        root_path: root.to_string_lossy().into_owned(),
        root_name: root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string_lossy().into_owned()),
        branch: git_output(&root, &["branch", "--show-current"]).filter(|branch| !branch.is_empty()),
        changes: workspace_changes(&root),
        truncated: files.len() >= max_entries,
        files,
    }
}

/// Async + off the UI thread + hard overall deadline. The blocking walk/git run
/// in `spawn_blocking`; if the whole thing exceeds the budget we still return a
/// minimal snapshot (root only) rather than letting the panel spin forever.
#[tauri::command]
pub async fn get_workspace_snapshot(workspace_path: String) -> Result<WorkspaceSnapshot, String> {
    let requested = PathBuf::from(workspace_path.trim());
    let root = requested
        .canonicalize()
        .map_err(|error| format!("工作区不存在或无法访问: {error}"))?;
    if !root.is_dir() {
        return Err("工作区路径不是文件夹".into());
    }

    let root_for_task = root.clone();
    let work = tokio::task::spawn_blocking(move || build_workspace_snapshot(root_for_task));
    // Budget covers file walk (≤3s) + up to two git calls (≤6s each) with margin.
    match tokio::time::timeout(std::time::Duration::from_secs(16), work).await {
        Ok(Ok(snapshot)) => Ok(snapshot),
        Ok(Err(_join_err)) => Err("读取工作区任务异常".into()),
        Err(_timeout) => Ok(WorkspaceSnapshot {
            root_path: root.to_string_lossy().into_owned(),
            root_name: root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned()),
            branch: None,
            changes: Vec::new(),
            truncated: true,
            files: Vec::new(),
        }),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FilePreview {
    pub path: String,
    pub kind: String, // text | markdown | image | pdf | binary
    pub language: String,
    pub content: String, // text content, or a data: URL for image/pdf
    pub size: u64,
    pub truncated: bool,
}

const TEXT_MAX: u64 = 512 * 1024;
const IMAGE_MAX: u64 = 8 * 1024 * 1024;
const PDF_MAX: u64 = 24 * 1024 * 1024;

fn image_mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

fn highlight_language(ext: &str) -> String {
    match ext {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "sh" | "bash" | "zsh" => "bash",
        "ps1" => "powershell",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "xml" | "svg" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "sql" => "sql",
        "md" | "markdown" => "markdown",
        other => other,
    }
    .to_string()
}

fn is_known_binary(ext: &str) -> bool {
    matches!(
        ext,
        "exe" | "dll" | "bin" | "so" | "dylib" | "a" | "o" | "obj" | "lib" | "class" | "pyc"
            | "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" | "xz" | "bz2"
            | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
            | "mp3" | "wav" | "flac" | "ogg" | "mp4" | "mov" | "avi" | "mkv" | "webm"
            | "woff" | "woff2" | "ttf" | "otf" | "eot"
            | "sqlite" | "db" | "wasm" | "node"
    )
}

#[tauri::command]
pub fn read_workspace_file(
    workspace_path: String,
    relative_path: String,
) -> Result<FilePreview, String> {
    let root = PathBuf::from(workspace_path.trim())
        .canonicalize()
        .map_err(|error| format!("工作区不存在或无法访问: {error}"))?;
    let target = root
        .join(relative_path.trim().replace('\\', "/"))
        .canonicalize()
        .map_err(|error| format!("文件不存在或无法访问: {error}"))?;
    // Path-traversal guard: the resolved file must stay inside the workspace.
    if !target.starts_with(&root) {
        return Err("文件超出工作区范围".into());
    }
    if !target.is_file() {
        return Err("所选路径不是文件".into());
    }

    let metadata = std::fs::metadata(&target).map_err(|error| error.to_string())?;
    let size = metadata.len();
    let ext = target
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    let rel = target
        .strip_prefix(&root)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| relative_path.clone());

    let binary = |kind: &str| FilePreview {
        path: rel.clone(),
        kind: kind.into(),
        language: String::new(),
        content: String::new(),
        size,
        truncated: false,
    };

    if let Some(mime) = image_mime(&ext) {
        if size > IMAGE_MAX {
            return Ok(binary("binary"));
        }
        let bytes = std::fs::read(&target).map_err(|error| error.to_string())?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        return Ok(FilePreview {
            path: rel,
            kind: "image".into(),
            language: String::new(),
            content: format!("data:{mime};base64,{encoded}"),
            size,
            truncated: false,
        });
    }

    if ext == "pdf" {
        if size > PDF_MAX {
            return Ok(binary("binary"));
        }
        let bytes = std::fs::read(&target).map_err(|error| error.to_string())?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        return Ok(FilePreview {
            path: rel,
            kind: "pdf".into(),
            language: String::new(),
            content: format!("data:application/pdf;base64,{encoded}"),
            size,
            truncated: false,
        });
    }

    if is_known_binary(&ext) {
        return Ok(binary("binary"));
    }

    // Text/code/markdown: read up to the cap and reject non-UTF-8 / null bytes.
    let read_len = (size.min(TEXT_MAX) + 1) as usize;
    let bytes = {
        use std::io::Read;
        let mut file = std::fs::File::open(&target).map_err(|error| error.to_string())?;
        let mut buffer = vec![0u8; read_len];
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        buffer.truncate(read);
        buffer
    };
    if bytes.contains(&0) {
        return Ok(binary("binary"));
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Ok(binary("binary")),
    };
    let truncated = size > TEXT_MAX;
    let kind = if ext == "md" || ext == "markdown" { "markdown" } else { "text" };
    Ok(FilePreview {
        path: rel,
        kind: kind.into(),
        language: highlight_language(&ext),
        content: text,
        size,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::{collect_workspace_files, read_workspace_file};

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

    #[test]
    fn read_text_file_and_block_traversal() {
        let root = std::env::temp_dir().join(format!(
            "omnix_preview_{}",
            chrono::Utc::now().timestamp_micros()
        ));
        std::fs::create_dir_all(root.join("src")).expect("dirs");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("file");

        let preview = read_workspace_file(
            root.to_string_lossy().into_owned(),
            "src/main.rs".into(),
        )
        .expect("preview");
        assert_eq!(preview.kind, "text");
        assert_eq!(preview.language, "rust");
        assert!(preview.content.contains("fn main"));

        // Path traversal must be rejected.
        let escaped = read_workspace_file(root.to_string_lossy().into_owned(), "../../etc/hosts".into());
        assert!(escaped.is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
