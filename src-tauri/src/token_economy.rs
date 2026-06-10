//! Token Economy & Agent Enhancements (DeepSeek-GUI inspired)
//!
//! 1. Token Economy — compress tool descriptions, cap result sizes
//! 2. Steering Queue — mid-turn user guidance injection
//! 3. Inline Diff Detection — track file modifications
//! 4. Plan/Goal workflow — structured task management
//! 5. SDD requirement workflow — draft → clarify → plan
//! 6. Progressive MCP discovery — search before inject

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
// 2. Steering Queue
// ══════════════════════════════════════════════════

/// A steering message injected mid-turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteeringMessage {
    pub id: String,
    pub content: String,
    pub timestamp: i64,
    pub consumed: bool,
}

/// Steering queue for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteeringQueue {
    pub session_id: String,
    pub messages: Vec<SteeringMessage>,
}

impl SteeringQueue {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            messages: Vec::new(),
        }
    }

    /// Enqueue a steering message
    pub fn push(&mut self, content: &str) -> String {
        let id = format!("steer_{}", chrono::Utc::now().timestamp_millis());
        self.messages.push(SteeringMessage {
            id: id.clone(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            consumed: false,
        });
        id
    }

    /// Consume all unconsumed messages
    pub fn drain_unconsumed(&mut self) -> Vec<SteeringMessage> {
        let mut result = Vec::new();
        for msg in &mut self.messages {
            if !msg.consumed {
                msg.consumed = true;
                result.push(msg.clone());
            }
        }
        result
    }

    /// Build steering context string for injection into prompt
    pub fn build_steering_context(&self) -> String {
        let unconsumed: Vec<&SteeringMessage> = self.messages.iter()
            .filter(|m| !m.consumed)
            .collect();

        if unconsumed.is_empty() {
            return String::new();
        }

        let mut ctx = String::from("\n\n<user_steering>\nThe user has injected the following guidance while you were working:\n");
        for msg in &unconsumed {
            ctx.push_str(&format!("- {}\n", msg.content));
        }
        ctx.push_str("</user_steering>\n");
        ctx
    }
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

// ══════════════════════════════════════════════════
// 4. Plan / Goal Workflow
// ══════════════════════════════════════════════════

/// A plan generated for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPlan {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub steps: Vec<PlanStep>,
    pub status: String,  // "draft" | "active" | "completed"
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub index: u32,
    pub description: String,
    pub status: String,  // "pending" | "in_progress" | "done" | "skipped"
    pub notes: Option<String>,
}

/// A persistent goal for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGoal {
    pub session_id: String,
    pub goal: String,
    pub set_at: i64,
    pub achieved: bool,
}

// ══════════════════════════════════════════════════
// 5. SDD Requirement Workflow
// ══════════════════════════════════════════════════

/// A software design document requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SddRequirement {
    pub id: String,
    pub title: String,
    pub background: String,
    pub goals: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub clarifications: Vec<SddClarification>,
    pub status: String,  // "draft" | "clarified" | "planned"
    pub plan_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SddClarification {
    pub question: String,
    pub answer: Option<String>,
}

/// Build an SDD prompt for AI clarification
pub fn build_sdd_clarification_prompt(req: &SddRequirement) -> String {
    let goals = req.goals.iter().map(|g| format!("- {}", g)).collect::<Vec<_>>().join("\n");
    let criteria = req.acceptance_criteria.iter().map(|c| format!("- {}", c)).collect::<Vec<_>>().join("\n");

    format!(
        r#"You are reviewing a software requirement. Ask clarifying questions to fill gaps.

## Title: {title}
## Background: {background}
## Goals:
{goals}
## Acceptance Criteria:
{criteria}

Ask 3-5 specific clarifying questions about:
1. Edge cases not covered
2. Non-functional requirements (performance, security, accessibility)
3. Dependencies on other systems
4. Scope boundaries (what's explicitly NOT included)

Return questions as a JSON array of strings."#,
        title = req.title,
        background = req.background,
        goals = goals,
        criteria = criteria,
    )
}

/// Build an SDD plan generation prompt
pub fn build_sdd_plan_prompt(req: &SddRequirement) -> String {
    let clarifications: String = req.clarifications.iter()
        .filter(|c| c.answer.is_some())
        .map(|c| format!("Q: {}\nA: {}", c.question, c.answer.as_ref().unwrap()))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"Generate an implementation plan for this requirement.

## Title: {title}
## Background: {background}
## Clarifications:
{clarifications}

Generate a step-by-step implementation plan with:
1. Each step should be a concrete, actionable task
2. Include file paths where changes are needed
3. Estimate complexity (S/M/L) for each step
4. Note dependencies between steps

Return as JSON array: [{{"index": 1, "description": "...", "complexity": "S"}}]"#,
        title = req.title,
        background = req.background,
        clarifications = clarifications,
    )
}

// ══════════════════════════════════════════════════
// 6. Progressive MCP Discovery
// ══════════════════════════════════════════════════

/// Search result for MCP tool discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolSearchResult {
    pub tool_name: String,
    pub server_name: String,
    pub description: String,
    pub relevance_score: f32,
}

/// Build an MCP tool search prompt for the agent
pub fn build_mcp_search_prompt(user_message: &str, available_tools: &[String]) -> String {
    let tool_list = available_tools.iter()
        .map(|t| format!("- {}", t))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"The user wants to: {message}

Available MCP tools:
{tools}

Which 1-3 tools are most relevant? Return ONLY tool names as JSON array: ["tool1", "tool2"]"#,
        message = user_message,
        tools = tool_list,
    )
}
