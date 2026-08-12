//! Input validation for Tauri command parameters.
//!
//! Provides reusable validation for:
//! - IDs (conversation_id, session_id, platform_id, etc.)
//! - Names (agent_name, skill_name, etc.)
//! - File paths (workspace_path, file_path, etc.)
//!
//! All Tauri commands that accept String parameters from the frontend
//! should validate them before use to prevent:
//! - Empty/whitespace-only values causing silent failures
//! - Overly long values (DoS / buffer concerns)
//! - Control characters (injection / log forging)
//! - Path traversal (reading/writing outside intended directories)

/// Maximum length for ID strings (UUIDs are 36 chars, our custom IDs are similar)
const MAX_ID_LEN: usize = 256;
/// Maximum length for name strings
const MAX_NAME_LEN: usize = 256;
/// Maximum length for general string content (titles, descriptions, etc.)
const MAX_CONTENT_LEN: usize = 65536;

// ── ID Validation ──────────────────────────────────────────

/// Validate an ID parameter (conversation_id, session_id, platform_id, task_id, etc.)
///
/// Rejects:
/// - Empty or whitespace-only strings
/// - Strings longer than 256 characters
/// - Strings containing control characters (except tab/newline for content)
pub fn validate_id(id: &str, param_name: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err(format!("{} must not be empty", param_name));
    }
    if id.len() > MAX_ID_LEN {
        return Err(format!("{} exceeds maximum length of {} characters", param_name, MAX_ID_LEN));
    }
    if contains_control_chars(id) {
        return Err(format!("{} contains invalid control characters", param_name));
    }
    Ok(())
}

/// Validate a name parameter (agent_name, skill_name, tool_id, etc.)
///
/// Same rules as validate_id but with a different error label.
pub fn validate_name(name: &str, param_name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{} must not be empty", param_name));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(format!("{} exceeds maximum length of {} characters", param_name, MAX_NAME_LEN));
    }
    if contains_control_chars(name) {
        return Err(format!("{} contains invalid control characters", param_name));
    }
    Ok(())
}

/// Validate general content (titles, descriptions, query text, etc.)
///
/// Less strict than ID/name — allows longer strings but still rejects
/// extremely long values and control characters.
pub fn validate_content(content: &str, param_name: &str) -> Result<(), String> {
    if content.len() > MAX_CONTENT_LEN {
        return Err(format!("{} exceeds maximum length of {} characters", param_name, MAX_CONTENT_LEN));
    }
    Ok(())
}

/// Validate a caller-supplied string that will be used as a **single path
/// component** — a file or directory name such as a note id (`<id>.md`) or a
/// skill name used as a subdirectory. Layers on top of [`validate_name`] and
/// additionally rejects path separators, drive/ADS colons, and `.`/`..`, so the
/// value cannot escape the directory it is joined onto.
pub fn validate_path_component(name: &str, param_name: &str) -> Result<(), String> {
    validate_name(name, param_name)?;
    let trimmed = name.trim();
    if trimmed == "." || trimmed == ".." {
        return Err(format!("{} must not be '.' or '..'", param_name));
    }
    if name.contains('/') || name.contains('\\') || name.contains(':') || name.contains('\0') {
        return Err(format!("{} must not contain path separators", param_name));
    }
    Ok(())
}

// ── Path Validation ────────────────────────────────────────

/// Validate a relative file path to prevent directory traversal attacks.
///
/// Rejects:
/// - Absolute paths (must be relative to a workspace)
/// - Paths containing `..` (parent directory traversal)
/// - Paths that resolve to system directories via symlink
pub fn validate_relative_path(path: &std::path::Path) -> Result<(), String> {
    // Reject absolute paths
    if path.is_absolute() {
        return Err("Absolute paths are not allowed for security reasons".to_string());
    }
    // Reject path traversal AND rooted/prefixed components. Note `..` and a
    // leading `/` (RootDir) or `C:\` (Prefix) all let a "relative" string escape
    // its base once joined — on Windows `base.join("/etc/x")` jumps to the drive
    // root even though `Path::is_absolute()` returns false for `/etc/x`.
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                return Err("Path traversal (..) is not allowed for security reasons".to_string());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("Rooted or absolute path segments are not allowed".to_string());
            }
            _ => {}
        }
    }
    // Reject if canonicalized path escapes to system directories
    if path.exists() {
        if let Ok(canonical) = std::fs::canonicalize(path) {
            check_system_directory(&canonical)?;
        }
    }
    Ok(())
}

/// Validate a workspace/project path (absolute paths ARE allowed here,
/// but system directories are not).
///
/// Use this for workspace_path, project_path, workspace_dir, etc.
/// These are user-specified project directories that should be absolute.
///
/// Rejects:
/// - Empty or whitespace-only strings
/// - Paths that resolve to system directories
/// - Paths containing `..` that escape above the specified root
pub fn validate_workspace_path(path: &str, param_name: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(format!("{} must not be empty", param_name));
    }
    if path.len() > 4096 {
        return Err(format!("{} exceeds maximum path length", param_name));
    }

    let p = std::path::Path::new(path);

    // 存在就用真实解析（能跟穿符号链接），不存在就按字面归一化。
    //
    // 以前这里是「`..` 出现超过 3 次就拒」+「`p.exists()` 时才检查」。两条都不成立：
    // `../../../etc/passwd` 正好 3 个，直接放行；而**要写的文件本来就不存在**，
    // 于是最需要检查的那一类路径从来没被检查过。
    let resolved = std::fs::canonicalize(p)
        .unwrap_or_else(|_| lexically_normalize(p));
    check_system_directory(&resolved)?;

    Ok(())
}

/// 不碰文件系统地解析掉 `.` 和 `..`。
///
/// 给「还不存在的路径」用——`canonicalize` 对不存在的路径直接失败，而那恰恰是
/// 新建文件的情形。逐段消解，`..` 弹掉上一段；已经在根上的 `..` 丢弃（和内核
/// 行为一致，`/..` 就是 `/`）。
fn lexically_normalize(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                // 只弹普通段：弹掉根或盘符会把 `/..` 变成相对路径。
                if out.components().next_back().is_some_and(|c| matches!(c, Component::Normal(_))) {
                    out.pop();
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ── Internal Helpers ───────────────────────────────────────

/// Check if a string contains ASCII control characters (0x00-0x1F except 0x0A newline, 0x0D carriage return, 0x09 tab)
fn contains_control_chars(s: &str) -> bool {
    s.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
}

/// 拒绝指向系统目录、或用户主目录下敏感位置的路径。
///
/// 两类分开处理：系统目录按绝对前缀匹配；主目录下的敏感位置要先拼出真实位置
/// 再比——写死 `~/.ssh` 没用，`~` 在这一层已经被展开成真实路径了。
fn check_system_directory(canonical: &std::path::Path) -> Result<(), String> {
    let path_str = compare_form(canonical);
    // 前缀一律写成 `/` 分隔的小写形式——`compare_form` 已经把两边拉成同一形态。
    let forbidden_prefixes = [
        "/etc/", "/proc/", "/sys/", "/dev/", "/root/", "/boot/",
        "c:/windows/", "c:/program files/", "c:/programdata/",
        "/windows/", "/program files/", "/programdata/",
    ];
    for prefix in &forbidden_prefixes {
        if path_str.starts_with(prefix) {
            return Err(format!("Access to system directory is not allowed: {}", prefix));
        }
    }

    // 主目录下的这几个，泄漏代价和系统目录一样高，而原来的黑名单一个都没盖到。
    // `.omnix` 排在最前面是因为**加密密钥就在里面**（`~/.omnix/.encryption_key`）——
    // 一个能读它的工作区，等于所有加密存储都白做了。
    if let Some(home) = dirs::home_dir() {
        for sensitive in [".omnix", ".ssh", ".aws", ".gnupg", ".kube", ".docker", ".config/gh"] {
            let guarded = compare_form(&home.join(sensitive));
            if path_str == guarded || path_str.starts_with(&format!("{guarded}/")) {
                return Err(format!(
                    "不允许把 {} 当作工作区——那里放着密钥或凭据",
                    home.join(sensitive).display()
                ));
            }
        }
    }
    Ok(())
}

/// 把路径拉成可比较的统一形态：`/` 分隔、小写、去掉 Windows 的 `\\?\` 扩展前缀。
///
/// 三件事都必要，而且都是被测试逼出来的：
/// - `Path::components()` 在 Windows 上把 `/etc` 归一成 `\etc`，拿去和 `/etc/`
///   比永远不匹配——原来的黑名单在 Windows 上对 POSIX 路径根本没生效过；
/// - `canonicalize` 在 Windows 返回 `\\?\C:\Users\…`，和 `home.join(..)` 拼出来
///   的 `C:\Users\…` 不相等；
/// - Windows 路径不区分大小写，`C:\WINDOWS\` 得能拦住。
fn compare_form(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let stripped = raw.strip_prefix("//?/UNC/").map(|rest| format!("//{rest}"))
        .or_else(|| raw.strip_prefix("//?/").map(str::to_string))
        .unwrap_or(raw);
    stripped.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_id_ok() {
        assert!(validate_id("conv_123", "conversation_id").is_ok());
        assert!(validate_id("550e8400-e29b-41d4-a716-446655440000", "id").is_ok());
    }

    #[test]
    fn test_validate_id_empty() {
        assert!(validate_id("", "id").is_err());
        assert!(validate_id("   ", "id").is_err());
    }

    #[test]
    fn test_validate_id_too_long() {
        let long = "x".repeat(300);
        assert!(validate_id(&long, "id").is_err());
    }

    #[test]
    fn test_validate_id_control_chars() {
        assert!(validate_id("id\x00evil", "id").is_err());
        assert!(validate_id("id\x1Bescape", "id").is_err());
    }

    #[test]
    fn test_validate_name_ok() {
        assert!(validate_name("claude-code", "agent_name").is_ok());
        assert!(validate_name("my_skill", "skill_name").is_ok());
    }

    #[test]
    fn test_validate_workspace_path_ok() {
        // These should pass (assuming they exist or at least don't hit system dirs)
        assert!(validate_workspace_path("/home/user/project", "workspace_path").is_ok());
        assert!(validate_workspace_path("C:\\Users\\dev\\project", "workspace_path").is_ok());
    }

    #[test]
    fn test_validate_workspace_path_empty() {
        assert!(validate_workspace_path("", "workspace_path").is_err());
        assert!(validate_workspace_path("   ", "workspace_path").is_err());
    }

    #[test]
    fn test_validate_relative_path_rejects_absolute() {
        // On Windows, Unix-style /etc/passwd is not "absolute" per Path::is_absolute(),
        // so use platform-appropriate absolute paths
        #[cfg(unix)]
        {
            assert!(validate_relative_path(std::path::Path::new("/etc/passwd")).is_err());
        }
        #[cfg(windows)]
        {
            assert!(validate_relative_path(std::path::Path::new("C:\\Windows\\System32")).is_err());
        }
    }

    #[test]
    fn test_validate_relative_path_rejects_traversal() {
        assert!(validate_relative_path(std::path::Path::new("../../etc/passwd")).is_err());
        assert!(validate_relative_path(std::path::Path::new("../../../root")).is_err());
    }

    #[test]
    fn test_validate_relative_path_ok() {
        assert!(validate_relative_path(std::path::Path::new("src/main.rs")).is_ok());
        assert!(validate_relative_path(std::path::Path::new("docs/readme.md")).is_ok());
    }

    #[test]
    fn test_validate_relative_path_rejects_rooted() {
        // Leading `/` has no drive prefix, so is_absolute() is false on Windows,
        // but RootDir must still be rejected (join escapes to the drive root).
        assert!(validate_relative_path(std::path::Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn test_validate_path_component() {
        assert!(validate_path_component("note_123", "id").is_ok());
        assert!(validate_path_component("my-skill", "name").is_ok());
        assert!(validate_path_component("../evil", "id").is_err());
        assert!(validate_path_component("a/b", "id").is_err());
        assert!(validate_path_component("a\\b", "id").is_err());
        assert!(validate_path_component("C:evil", "id").is_err());
        assert!(validate_path_component("..", "id").is_err());
        assert!(validate_path_component("", "id").is_err());
    }
}

#[cfg(test)]
mod path_closure_tests {
    use super::*;

    /// 旧实现是「`..` 超过 3 次就拒」+「路径存在时才检查」。这两条各自漏掉的
    /// 东西，正好是最该拦的两类。
    #[test]
    fn traversal_that_slipped_through_the_old_heuristic_is_now_refused() {
        // 正好 3 个 `..`——旧的计数阈值放行，而它明明落在 /etc。
        assert!(validate_workspace_path("/a/b/c/../../../etc/passwd", "p").is_err());
        assert!(validate_workspace_path("/x/../etc/shadow", "p").is_err());
        assert!(
            validate_workspace_path(r"C:\Users\me\..\..\Windows\System32", "p").is_err(),
            "Windows 上同样要拦"
        );
    }

    /// **不存在的路径也必须检查。** 要写的文件本来就不存在，而旧实现对它们
    /// 一律放行——最该管的那一类反而完全没管。
    #[test]
    fn nonexistent_paths_are_checked_too() {
        let ghost = "/etc/definitely-not-here-omnix-test/sub/file.txt";
        assert!(!std::path::Path::new(ghost).exists(), "这条用例要求路径不存在");
        assert!(validate_workspace_path(ghost, "p").is_err(), "不存在也要拦");
    }

    /// 主目录下的密钥/凭据目录。`.omnix` 尤其重要——加密密钥就在里面，
    /// 能读它等于所有加密存储白做。
    #[test]
    fn sensitive_home_directories_are_refused() {
        let Some(home) = dirs::home_dir() else { return };
        for sensitive in [".omnix", ".ssh", ".aws", ".gnupg"] {
            let path = home.join(sensitive);
            assert!(
                validate_workspace_path(&path.to_string_lossy(), "p").is_err(),
                "{} 应被拒绝",
                path.display()
            );
            // 子路径同样要拦，不能只拦目录本身。
            let child = path.join("something");
            assert!(
                validate_workspace_path(&child.to_string_lossy(), "p").is_err(),
                "{} 应被拒绝",
                child.display()
            );
        }
    }

    /// 正常的工作区不能被误伤——过度拦截会把功能拦死，比漏拦更快被发现，
    /// 但同样是 bug。
    #[test]
    fn ordinary_workspaces_still_pass() {
        let Some(home) = dirs::home_dir() else { return };
        for ok in [
            home.join("projects").join("myapp"),
            home.join("Documents").join("notes"),
            std::path::PathBuf::from(r"D:\Agent\Project\OMNIX-Workbench"),
        ] {
            assert!(
                validate_workspace_path(&ok.to_string_lossy(), "p").is_ok(),
                "{} 不该被拒绝：{:?}",
                ok.display(),
                validate_workspace_path(&ok.to_string_lossy(), "p")
            );
        }
    }

    /// `..` 消解到根上就停，不能把绝对路径变成相对路径。
    #[test]
    fn parent_components_never_escape_the_root() {
        let normalized = lexically_normalize(std::path::Path::new("/../../../etc"));
        assert_eq!(normalized, std::path::PathBuf::from("/etc"), "{normalized:?}");
    }
}
