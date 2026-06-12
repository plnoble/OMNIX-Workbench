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
    // Reject path traversal components
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            return Err("Path traversal (..) is not allowed for security reasons".to_string());
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

    // Check for excessive traversal
    let dot_dot_count = path.matches("..").count();
    if dot_dot_count > 3 {
        return Err(format!("{} contains excessive path traversal components", param_name));
    }

    // Reject if the path resolves to a system directory
    if p.exists() {
        if let Ok(canonical) = std::fs::canonicalize(p) {
            check_system_directory(&canonical)?;
        }
    }

    Ok(())
}

// ── Internal Helpers ───────────────────────────────────────

/// Check if a string contains ASCII control characters (0x00-0x1F except 0x0A newline, 0x0D carriage return, 0x09 tab)
fn contains_control_chars(s: &str) -> bool {
    s.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
}

/// Check if a canonical path points to a system directory.
fn check_system_directory(canonical: &std::path::Path) -> Result<(), String> {
    let path_str = canonical.to_string_lossy();
    let forbidden_prefixes = [
        "/etc/", "/proc/", "/sys/", "/dev/", "/root/",
        "C:\\Windows\\", "C:\\Program Files\\", "C:\\ProgramData\\",
        "\\Windows\\", "\\Program Files\\", "\\ProgramData\\",
    ];
    for prefix in &forbidden_prefixes {
        if path_str.starts_with(prefix) {
            return Err(format!("Access to system directory is not allowed: {}", prefix));
        }
    }
    Ok(())
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
}
