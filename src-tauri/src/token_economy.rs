//! Token Economy & Agent Enhancements
//!
//! 1. Token Economy — compress tool descriptions, cap result sizes
//! 2. Inline Diff Detection — track file modifications
//!
//! (Steering queue / plan-goal / SDD / MCP-discovery prompt scaffolding lived
//! here as staged code but was never wired up; removed 2026-07 — recover from
//! git history if a future feature needs it.)

use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════
// 1. Token Economy
// ══════════════════════════════════════════════════

/// Token budget configuration for a request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Max tokens for tool descriptions in system prompt
    pub max_tool_desc_tokens: u32,
    /// Max lines per tool result
    pub max_tool_result_lines: u32,
    /// Max bytes per tool result
    pub max_tool_result_bytes: u32,
    /// Max history messages to include
    pub max_history_messages: u32,
    /// Enable aggressive compression for tool results
    pub compress_tool_results: bool,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            max_tool_desc_tokens: 2000,
            max_tool_result_lines: 100,
            max_tool_result_bytes: 10000,
            max_history_messages: 50,
            compress_tool_results: true,
        }
    }
}

/// Compress a tool result to fit within budget
pub fn compress_tool_result(content: &str, budget: &TokenBudget) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Truncate by line count
    let truncated = if lines.len() > budget.max_tool_result_lines as usize {
        let mut result: Vec<&str> = lines[..budget.max_tool_result_lines as usize].to_vec();
        result.push(&"... (truncated)");
        result.join("\n")
    } else {
        content.to_string()
    };

    // Truncate by byte size
    if truncated.len() > budget.max_tool_result_bytes as usize {
        let end = truncated
            .char_indices()
            .nth(budget.max_tool_result_bytes as usize)
            .map(|(i, _)| i)
            .unwrap_or(truncated.len());
        format!("{}... (truncated)", &truncated[..end])
    } else {
        truncated
    }
}

/// Estimate token count (rough: 4 chars ≈ 1 token for English)
pub fn estimate_tokens(text: &str) -> u32 {
    let ascii = text.chars().filter(|c| c.is_ascii()).count() as u32;
    let cjk = text.chars().filter(|c| !c.is_ascii()).count() as u32;
    ascii / 4 + cjk / 2 + 1
}




// ══════════════════════════════════════════════════
// 3. Inline Diff Detection
// ══════════════════════════════════════════════════

/// A detected file modification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub file_path: String,
    pub change_type: String,  // "created" | "modified" | "deleted"
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub diff_summary: String,
    pub timestamp: i64,
}

/// Detect changes between old and new content
pub fn detect_file_change(
    file_path: &str,
    old_content: Option<&str>,
    new_content: Option<&str>,
) -> FileChange {
    let change_type = match (old_content, new_content) {
        (None, Some(_)) => "created",
        (Some(_), None) => "deleted",
        (Some(_), Some(_)) => "modified",
        (None, None) => "unchanged",
    };

    let diff_summary = match (old_content, new_content) {
        (Some(old), Some(new)) => {
            let old_lines: Vec<&str> = old.lines().collect();
            let new_lines: Vec<&str> = new.lines().collect();
            let added = new_lines.iter().filter(|l| !old_lines.contains(l)).count();
            let removed = old_lines.iter().filter(|l| !new_lines.contains(l)).count();
            format!("+{} -{} lines", added, removed)
        }
        (None, Some(new)) => format!("+{} lines", new.lines().count()),
        (Some(old), None) => format!("-{} lines", old.lines().count()),
        _ => "no changes".into(),
    };

    FileChange {
        file_path: file_path.to_string(),
        change_type: change_type.to_string(),
        old_content: old_content.map(|s| s.to_string()),
        new_content: new_content.map(|s| s.to_string()),
        diff_summary,
        timestamp: chrono::Utc::now().timestamp(),
    }
}