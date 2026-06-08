//! Tool Adapters for Skill Sync System
//!
//! Each AI coding tool has its own skill directory convention.
//! This module provides a unified trait for detecting tools,
//! discovering their skill directories, and syncing skill files.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// How a skill file is synced to a tool's directory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncMode {
    /// Copy the file content
    Copy,
    /// Create a symbolic link
    Symlink,
}

impl std::fmt::Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncMode::Copy => write!(f, "copy"),
            SyncMode::Symlink => write!(f, "symlink"),
        }
    }
}

/// Status of a skill sync target
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    Pending,
    Synced,
    Error,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStatus::Pending => write!(f, "pending"),
            SyncStatus::Synced => write!(f, "synced"),
            SyncStatus::Error => write!(f, "error"),
        }
    }
}

/// Installation status of a tool
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolInstallStatus {
    Installed,
    NotInstalled,
    Partial,
}

/// A skill file discovered in a tool's skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    pub name: String,
    pub path: String,
    pub tool: String,
    pub content_hash: String,
}

/// Result of a sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub tool: String,
    pub target_path: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Tool adapter trait — each AI tool implements this
pub trait ToolAdapter: Send + Sync {
    /// Unique identifier for this tool (e.g., "claude_code")
    fn tool_id(&self) -> &str;

    /// Human-readable display name (e.g., "Claude Code")
    fn display_name(&self) -> &str;

    /// Whether this tool is installed on the system
    fn is_installed(&self) -> bool;

    /// Base directory where this tool stores skills
    fn skill_base_path(&self) -> Option<PathBuf>;

    /// Sync a skill file to this tool's directory
    fn sync_skill(&self, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult;

    /// Remove a skill from this tool's directory
    fn unsync_skill(&self, skill_name: &str) -> SyncResult;

    /// List all skills currently in this tool's directory
    fn list_skills(&self) -> Vec<DiscoveredSkill>;
}

// ─────────────────────────────────────────────
// Core Adapter Implementations
// ─────────────────────────────────────────────

/// Claude Code adapter — ~/.claude/skills/<name>/SKILL.md
pub struct ClaudeCodeAdapter;

impl ToolAdapter for ClaudeCodeAdapter {
    fn tool_id(&self) -> &str { "claude_code" }
    fn display_name(&self) -> &str { "Claude Code" }

    fn is_installed(&self) -> bool {
        which::which("claude").is_ok()
    }

    fn skill_base_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(home.join(".claude").join("skills"))
    }

    fn sync_skill(&self, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return SyncResult {
                tool: self.tool_id().to_string(),
                target_path: String::new(),
                success: false,
                error: Some("Cannot determine home directory".to_string()),
            },
        };

        let target_dir = base.join(skill_name);
        let target_file = target_dir.join("SKILL.md");

        // Ensure directory exists
        if let Err(e) = fs::create_dir_all(&target_dir) {
            return SyncResult {
                tool: self.tool_id().to_string(),
                target_path: target_file.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("Failed to create directory: {}", e)),
            };
        }

        match mode {
            SyncMode::Copy => {
                if let Err(e) = atomic_write(&target_file, content) {
                    return SyncResult {
                        tool: self.tool_id().to_string(),
                        target_path: target_file.to_string_lossy().to_string(),
                        success: false,
                        error: Some(format!("Failed to write file: {}", e)),
                    };
                }
            }
            SyncMode::Symlink => {
                // For symlink mode, we create a symlink from the tool's skill dir
                // to the central storage path. The caller must ensure the source exists.
                if target_file.exists() || target_file.is_symlink() {
                    let _ = fs::remove_file(&target_file);
                }
                #[cfg(windows)]
                {
                    // Windows requires elevated privileges for symlinks,
                    // fall back to copy if symlink fails
                    if std::os::windows::fs::symlink_file(&target_file, &target_file).is_err() {
                        if let Err(e) = atomic_write(&target_file, content) {
                            return SyncResult {
                                tool: self.tool_id().to_string(),
                                target_path: target_file.to_string_lossy().to_string(),
                                success: false,
                                error: Some(format!("Symlink failed, copy fallback failed: {}", e)),
                            };
                        }
                    }
                }
                #[cfg(not(windows))]
                {
                    if let Err(e) = std::os::unix::fs::symlink(&target_file, &target_file) {
                        if let Err(e2) = atomic_write(&target_file, content) {
                            return SyncResult {
                                tool: self.tool_id().to_string(),
                                target_path: target_file.to_string_lossy().to_string(),
                                success: false,
                                error: Some(format!("Symlink failed, copy fallback failed: {} / {}", e, e2)),
                            };
                        }
                    }
                }
            }
        }

        SyncResult {
            tool: self.tool_id().to_string(),
            target_path: target_file.to_string_lossy().to_string(),
            success: true,
            error: None,
        }
    }

    fn unsync_skill(&self, skill_name: &str) -> SyncResult {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return SyncResult {
                tool: self.tool_id().to_string(),
                target_path: String::new(),
                success: false,
                error: Some("Cannot determine home directory".to_string()),
            },
        };

        let target_dir = base.join(skill_name);
        if target_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&target_dir) {
                return SyncResult {
                    tool: self.tool_id().to_string(),
                    target_path: target_dir.to_string_lossy().to_string(),
                    success: false,
                    error: Some(format!("Failed to remove: {}", e)),
                };
            }
        }

        SyncResult {
            tool: self.tool_id().to_string(),
            target_path: target_dir.to_string_lossy().to_string(),
            success: true,
            error: None,
        }
    }

    fn list_skills(&self) -> Vec<DiscoveredSkill> {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return Vec::new(),
        };

        scan_skill_dir(&base, self.tool_id())
    }
}

/// Cursor adapter — ~/.cursor/skills/<name>/SKILL.md
pub struct CursorAdapter;

impl ToolAdapter for CursorAdapter {
    fn tool_id(&self) -> &str { "cursor" }
    fn display_name(&self) -> &str { "Cursor" }

    fn is_installed(&self) -> bool {
        // Check common Cursor installation paths on Windows
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        if !local_app_data.is_empty() {
            let cursor_path = PathBuf::from(&local_app_data).join("Programs").join("cursor");
            if cursor_path.exists() {
                return true;
            }
        }
        // Fallback: check PATH
        which::which("cursor").is_ok()
    }

    fn skill_base_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(home.join(".cursor").join("skills"))
    }

    fn sync_skill(&self, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult {
        generic_sync_skill(self, skill_name, content, mode)
    }

    fn unsync_skill(&self, skill_name: &str) -> SyncResult {
        generic_unsync_skill(self, skill_name)
    }

    fn list_skills(&self) -> Vec<DiscoveredSkill> {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return Vec::new(),
        };
        scan_skill_dir(&base, self.tool_id())
    }
}

/// GitHub Copilot adapter — .github/copilot/skills/<name>/SKILL.md (project-level)
pub struct CopilotAdapter;

impl ToolAdapter for CopilotAdapter {
    fn tool_id(&self) -> &str { "copilot" }
    fn display_name(&self) -> &str { "GitHub Copilot" }

    fn is_installed(&self) -> bool {
        // Copilot is a VS Code extension, check VS Code extensions dir
        let home = dirs::home_dir().unwrap_or_default();
        let ext_path = home.join(".vscode").join("extensions");
        if ext_path.exists() {
            if let Ok(entries) = fs::read_dir(&ext_path) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("github.copilot") {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn skill_base_path(&self) -> Option<PathBuf> {
        // Copilot uses project-level .github/copilot/skills/
        // We use a global storage path instead
        let home = dirs::home_dir()?;
        Some(home.join(".github").join("copilot").join("skills"))
    }

    fn sync_skill(&self, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult {
        generic_sync_skill(self, skill_name, content, mode)
    }

    fn unsync_skill(&self, skill_name: &str) -> SyncResult {
        generic_unsync_skill(self, skill_name)
    }

    fn list_skills(&self) -> Vec<DiscoveredSkill> {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return Vec::new(),
        };
        scan_skill_dir(&base, self.tool_id())
    }
}

/// Gemini CLI adapter — ~/.gemini/skills/<name>/SKILL.md
pub struct GeminiCliAdapter;

impl ToolAdapter for GeminiCliAdapter {
    fn tool_id(&self) -> &str { "gemini_cli" }
    fn display_name(&self) -> &str { "Gemini CLI" }

    fn is_installed(&self) -> bool {
        which::which("gemini").is_ok()
    }

    fn skill_base_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(home.join(".gemini").join("skills"))
    }

    fn sync_skill(&self, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult {
        generic_sync_skill(self, skill_name, content, mode)
    }

    fn unsync_skill(&self, skill_name: &str) -> SyncResult {
        generic_unsync_skill(self, skill_name)
    }

    fn list_skills(&self) -> Vec<DiscoveredSkill> {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return Vec::new(),
        };
        scan_skill_dir(&base, self.tool_id())
    }
}

/// Codex (OpenAI) adapter — ~/.codex/skills/<name>/SKILL.md
pub struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn tool_id(&self) -> &str { "codex" }
    fn display_name(&self) -> &str { "Codex" }

    fn is_installed(&self) -> bool {
        which::which("codex").is_ok()
    }

    fn skill_base_path(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(home.join(".codex").join("skills"))
    }

    fn sync_skill(&self, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult {
        generic_sync_skill(self, skill_name, content, mode)
    }

    fn unsync_skill(&self, skill_name: &str) -> SyncResult {
        generic_unsync_skill(self, skill_name)
    }

    fn list_skills(&self) -> Vec<DiscoveredSkill> {
        let base = match self.skill_base_path() {
            Some(p) => p,
            None => return Vec::new(),
        };
        scan_skill_dir(&base, self.tool_id())
    }
}

// ─────────────────────────────────────────────
// Adapter Registry
// ─────────────────────────────────────────────

/// Registry that holds all available tool adapters
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn ToolAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        let adapters: Vec<Box<dyn ToolAdapter>> = vec![
            Box::new(ClaudeCodeAdapter),
            Box::new(CursorAdapter),
            Box::new(CopilotAdapter),
            Box::new(GeminiCliAdapter),
            Box::new(CodexAdapter),
        ];
        Self { adapters }
    }

    /// Get all registered adapters
    pub fn all(&self) -> &[Box<dyn ToolAdapter>] {
        &self.adapters
    }

    /// Get a specific adapter by tool_id
    pub fn get(&self, tool_id: &str) -> Option<&Box<dyn ToolAdapter>> {
        self.adapters.iter().find(|a| a.tool_id() == tool_id)
    }

    /// Get all installed adapters
    #[allow(dead_code)]
    pub fn installed(&self) -> Vec<&Box<dyn ToolAdapter>> {
        self.adapters.iter().filter(|a| a.is_installed()).collect()
    }

    /// Get all tool status info for frontend display
    pub fn tool_status_list(&self) -> Vec<ToolStatus> {
        self.adapters.iter().map(|a| ToolStatus {
            tool_id: a.tool_id().to_string(),
            display_name: a.display_name().to_string(),
            is_installed: a.is_installed(),
            skill_base_path: a.skill_base_path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        }).collect()
    }
}

/// Tool status info returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool_id: String,
    pub display_name: String,
    pub is_installed: bool,
    pub skill_base_path: String,
}

// ─────────────────────────────────────────────
// Shared Helpers
// ─────────────────────────────────────────────

/// Generic sync implementation shared by adapters with standard skill directory layout
fn generic_sync_skill(adapter: &dyn ToolAdapter, skill_name: &str, content: &str, mode: &SyncMode) -> SyncResult {
    let base = match adapter.skill_base_path() {
        Some(p) => p,
        None => return SyncResult {
            tool: adapter.tool_id().to_string(),
            target_path: String::new(),
            success: false,
            error: Some("Cannot determine skill base path".to_string()),
        },
    };

    let target_dir = base.join(skill_name);
    let target_file = target_dir.join("SKILL.md");

    if let Err(e) = fs::create_dir_all(&target_dir) {
        return SyncResult {
            tool: adapter.tool_id().to_string(),
            target_path: target_file.to_string_lossy().to_string(),
            success: false,
            error: Some(format!("Failed to create directory: {}", e)),
        };
    }

    match mode {
        SyncMode::Copy => {
            if let Err(e) = atomic_write(&target_file, content) {
                return SyncResult {
                    tool: adapter.tool_id().to_string(),
                    target_path: target_file.to_string_lossy().to_string(),
                    success: false,
                    error: Some(format!("Failed to write file: {}", e)),
                };
            }
        }
        SyncMode::Symlink => {
            // Symlink mode: on Windows, fallback to copy (symlinks need elevated privileges)
            if let Err(e) = atomic_write(&target_file, content) {
                return SyncResult {
                    tool: adapter.tool_id().to_string(),
                    target_path: target_file.to_string_lossy().to_string(),
                    success: false,
                    error: Some(format!("Symlink not supported on Windows, copy failed: {}", e)),
                };
            }
        }
    }

    SyncResult {
        tool: adapter.tool_id().to_string(),
        target_path: target_file.to_string_lossy().to_string(),
        success: true,
        error: None,
    }
}

/// Generic unsync implementation
fn generic_unsync_skill(adapter: &dyn ToolAdapter, skill_name: &str) -> SyncResult {
    let base = match adapter.skill_base_path() {
        Some(p) => p,
        None => return SyncResult {
            tool: adapter.tool_id().to_string(),
            target_path: String::new(),
            success: false,
            error: Some("Cannot determine skill base path".to_string()),
        },
    };

    let target_dir = base.join(skill_name);
    if target_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&target_dir) {
            return SyncResult {
                tool: adapter.tool_id().to_string(),
                target_path: target_dir.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("Failed to remove: {}", e)),
                };
        }
    }

    SyncResult {
        tool: adapter.tool_id().to_string(),
        target_path: target_dir.to_string_lossy().to_string(),
        success: true,
        error: None,
    }
}

/// Scan a skill directory for SKILL.md files
fn scan_skill_dir(base: &PathBuf, tool_id: &str) -> Vec<DiscoveredSkill> {
    if !base.exists() {
        return Vec::new();
    }

    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let skill_dir = entry.path();
                let skill_file = skill_dir.join("SKILL.md");
                if skill_file.exists() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let hash = compute_file_hash(&skill_file);
                    results.push(DiscoveredSkill {
                        name,
                        path: skill_file.to_string_lossy().to_string(),
                        tool: tool_id.to_string(),
                        content_hash: hash,
                    });
                }
            }
        }
    }
    results
}

/// Atomic file write — write to temp file then rename
fn atomic_write(path: &PathBuf, content: &str) -> std::io::Result<()> {
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)
}

/// Compute content hash — uses file size + first/last bytes as fingerprint
/// TODO: Replace with proper SHA256 when sha2 crate is added
fn compute_file_hash(path: &PathBuf) -> String {
    match fs::read(path) {
        Ok(data) => {
            // Fingerprint: size + first 8 bytes + last 8 bytes as hex
            let len = data.len();
            let head = data.get(..8).unwrap_or(&[]);
            let tail = if len > 8 { data.get(len - 8..).unwrap_or(&[]) } else { &[] };
            format!(
                "fp-{:08x}-{}-{}",
                len as u32,
                head.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
                tail.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
            )
        }
        Err(_) => String::new(),
    }
}
