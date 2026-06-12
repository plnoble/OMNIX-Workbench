//! Skill Library Features (Skill Library inspired)
//!
//! 1. Semantic Skill Auto-Injection — match skills to user messages
//! 2. Dual-Agent Sandbox Testing — adversarial skill quality testing
//! 3. Text Protocol Interception — parse AI output for skill:/memory:/task: blocks
//! 4. Multi-Source Market Search — search GitHub/Anthropic for skills
//! 5. Experience Distillation — extract skills from project history

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::db::DbManager;

// ══════════════════════════════════════════════════
// 1. Semantic Skill Auto-Injection
// ══════════════════════════════════════════════════

/// Match result for a skill against user input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    pub skill_name: String,
    pub relevance_score: f32,
    pub matched_keywords: Vec<String>,
    pub content_preview: String,
}

/// Find skills that semantically match a user message.
/// Uses keyword matching against skill name, description, and category.
pub fn match_skills_for_message(db: &DbManager, message: &str) -> Vec<SkillMatch> {
    let conn = match db.get_connection() { Ok(c) => c, Err(_) => return Vec::new() };

    let mut stmt = match conn.prepare(
        "SELECT name, description, category, file_path FROM skills WHERE is_active = 1"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let skills: Vec<(String, String, Option<String>, String)> = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    }) {
        Ok(r) => r.flatten().collect(),
        Err(_) => return Vec::new(),
    };

    let message_lower = message.to_lowercase();
    let message_words: Vec<&str> = message_lower.split_whitespace().collect();
    let mut matches = Vec::new();

    for (name, description, category, file_path) in skills {
        let name_lower = name.to_lowercase();
        let desc_lower = description.to_lowercase();
        let cat_lower = category.as_deref().unwrap_or("").to_lowercase();

        let mut score = 0.0f32;
        let mut matched_keywords = Vec::new();

        // Direct name match (highest weight)
        if message_lower.contains(&name_lower) {
            score += 10.0;
            matched_keywords.push(name.clone());
        }

        // Keyword matching against description
        for word in &message_words {
            if word.len() < 3 { continue; } // Skip short words

            if desc_lower.contains(word) {
                score += 2.0;
                matched_keywords.push(word.to_string());
            }
            if cat_lower.contains(word) {
                score += 3.0;
                matched_keywords.push(format!("cat:{}", word));
            }
            if name_lower.contains(word) {
                score += 5.0;
                matched_keywords.push(format!("name:{}", word));
            }
        }

        // Category-based boosting
        let boost_keywords: Vec<(&str, &str)> = vec![
            ("bug", "调试诊断"), ("error", "调试诊断"), ("fix", "调试诊断"),
            ("code", "研发效能"), ("review", "研发效能"), ("test", "研发效能"),
            ("write", "文档办公"), ("doc", "文档办公"), ("translate", "文档办公"),
            ("security", "安全"), ("deploy", "部署"), ("git", "版本控制"),
            ("design", "设计"), ("ui", "设计"), ("api", "接口"),
        ];

        for (keyword, boost_cat) in &boost_keywords {
            if message_lower.contains(keyword) && (cat_lower.contains(boost_cat) || desc_lower.contains(keyword)) {
                score += 1.5;
            }
        }

        if score >= 3.0 {
            // Read first 200 chars of skill content for preview
            let preview = std::fs::read_to_string(
                PathBuf::from(&file_path).join(format!("{}_core.md", name))
            )
            .or_else(|_| std::fs::read_to_string(PathBuf::from(&file_path).join("SKILL.md")))
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();

            matches.push(SkillMatch {
                skill_name: name,
                relevance_score: score,
                matched_keywords,
                content_preview: preview,
            });
        }
    }

    // Sort by relevance descending
    matches.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
    matches.truncate(5); // Top 5 matches
    matches
}

/// Build a prompt injection string from matched skills
pub fn build_skill_injection(matches: &[SkillMatch], db: &DbManager) -> String {
    if matches.is_empty() {
        return String::new();
    }

    let conn = match db.get_connection() { Ok(c) => c, Err(_) => return String::new() };
    let mut injection = String::from("\n\n<auto_injected_skills>\nThe following skills are automatically activated based on the current task:\n\n");

    for m in matches {
        // Read full skill content
        let content: String = conn.query_row(
            "SELECT file_path FROM skills WHERE name = ?1",
            rusqlite::params![m.skill_name],
            |r| r.get(0),
        ).ok()
        .and_then(|fp: String| {
            let core = PathBuf::from(&fp).join(format!("{}_core.md", m.skill_name));
            std::fs::read_to_string(core).ok()
        })
        .unwrap_or_default();

        injection.push_str(&format!("## Skill: {} (relevance: {:.1})\n{}\n\n", m.skill_name, m.relevance_score, content));
    }

    injection.push_str("</auto_injected_skills>\n");
    injection
}

// ══════════════════════════════════════════════════
// 2. Dual-Agent Sandbox Testing
// ══════════════════════════════════════════════════

/// Test case for sandbox testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxTestCase {
    pub input: String,
    pub expected_behavior: String,
}

/// Result of a sandbox test run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub skill_name: String,
    pub test_cases_total: usize,
    pub test_cases_passed: usize,
    pub average_score: f32,
    pub scores: Vec<TestCaseScore>,
    pub overall_verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseScore {
    pub input: String,
    pub agent_response: String,
    pub auditor_score: u32,
    pub auditor_feedback: String,
}

/// Generate default test cases for a skill
pub fn generate_test_cases(skill_name: &str, skill_content: &str) -> Vec<SandboxTestCase> {
    // Generate basic test cases based on skill content
    let mut cases = Vec::new();

    // Test 1: Basic usage
    cases.push(SandboxTestCase {
        input: format!("Using the {} skill, explain your approach to a common task in your domain.", skill_name),
        expected_behavior: "Should demonstrate the skill's core knowledge and workflow".into(),
    });

    // Test 2: Edge case
    cases.push(SandboxTestCase {
        input: format!("Apply {} to handle an edge case or error scenario.", skill_name),
        expected_behavior: "Should identify the edge case and provide a structured solution".into(),
    });

    // Test 3: Anti-pattern detection
    if skill_content.contains("Anti-Pattern") || skill_content.contains("anti-pattern") || skill_content.contains("Do NOT") {
        cases.push(SandboxTestCase {
            input: format!("What should you NOT do when using the {} skill?", skill_name),
            expected_behavior: "Should correctly list the anti-patterns from the skill".into(),
        });
    }

    cases
}

/// Build the auditor prompt for scoring
pub fn build_auditor_prompt(skill_name: &str, skill_content: &str, test_input: &str, agent_response: &str) -> String {
    format!(
        r#"You are a strict quality auditor for AI skills. Evaluate the following response.

## Skill Being Tested: {skill_name}

## Skill Definition:
{skill_content}

## Test Input:
{test_input}

## Agent's Response:
{agent_response}

## Scoring Criteria (1-10):
1. **Relevance** (1-10): Does the response demonstrate the skill's domain knowledge?
2. **Accuracy** (1-10): Is the information technically correct?
3. **Completeness** (1-10): Does it cover the key aspects from the skill definition?
4. **Structure** (1-10): Does it follow the skill's workflow/checklist format?
5. **Anti-pattern avoidance** (1-10): Does it avoid the skill's listed anti-patterns?

Return ONLY a JSON object:
{{"score": <1-10>, "feedback": "<brief explanation>", "breakdown": {{"relevance": <1-10>, "accuracy": <1-10>, "completeness": <1-10>, "structure": <1-10>, "anti_pattern": <1-10>}}}}"#,
        skill_name = skill_name,
        skill_content = skill_content.chars().take(2000).collect::<String>(),
        test_input = test_input,
        agent_response = agent_response.chars().take(2000).collect::<String>(),
    )
}

// ══════════════════════════════════════════════════
// 3. Text Protocol Interception
// ══════════════════════════════════════════════════

/// Protocol action detected in AI output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolAction {
    pub action_type: String,  // "skill" | "memory" | "task" | "config"
    pub target: String,       // skill ID, memory key, task title, etc.
    pub content: String,      // payload
    pub raw_block: String,    // original code block
}

/// Parse protocol blocks from AI output text.
/// Looks for fenced code blocks with special language tags:
///   ```skill:ID/profile  ... ```
///   ```memory:add ... ```
///   ```task:add ... ```
pub fn intercept_protocols(output: &str) -> Vec<ProtocolAction> {
    let mut actions = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Look for fenced code blocks with protocol tags
        if line.starts_with("```") {
            let lang_tag = line[3..].trim();

            // Check for protocol tags
            if lang_tag.starts_with("skill:") || lang_tag.starts_with("memory:")
                || lang_tag.starts_with("task:") || lang_tag.starts_with("config:")
            {
                let action_type = lang_tag.split(':').next().unwrap_or("").to_string();
                let target = lang_tag.split(':').nth(1).unwrap_or("").to_string();

                // Collect block content
                let mut block_content = Vec::new();
                i += 1;
                while i < lines.len() && !lines[i].trim().starts_with("```") {
                    block_content.push(lines[i]);
                    i += 1;
                }

                let content = block_content.join("\n");
                let raw_block = format!("```{}\n{}\n```", lang_tag, content);

                actions.push(ProtocolAction {
                    action_type,
                    target,
                    content,
                    raw_block,
                });
            }
        }
        i += 1;
    }

    actions
}

/// Execute a protocol action
pub fn execute_protocol_action(
    action: &ProtocolAction,
    db: &DbManager,
) -> Result<String, String> {
    match action.action_type.as_str() {
        "memory" => {
            // Store memory
            let conn = db.get_connection().map_err(|e| e.to_string())?;
            let id = format!("proto_mem_{}", chrono::Utc::now().timestamp_millis());
            conn.execute(
                "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type) VALUES (?1, ?2, ?3, ?4, ?5, 'preference')",
                rusqlite::params![id, action.target, action.content, "", "auto-extracted"],
            ).map_err(|e| e.to_string())?;
            Ok(format!("Memory stored: {}", id))
        }
        "task" => {
            // Add to checklist (table created in init_schema)
            let conn = db.get_connection().map_err(|e| e.to_string())?;
            let id = format!("proto_chk_{}", chrono::Utc::now().timestamp_millis());
            conn.execute(
                "INSERT INTO dev_checklist (id, session_id, title, source) VALUES (?1, 'protocol', ?2, 'ai_generated')",
                rusqlite::params![id, action.content],
            ).map_err(|e| e.to_string())?;
            Ok(format!("Task added: {}", id))
        }
        _ => Ok(format!("Unknown protocol action: {}", action.action_type)),
    }
}

// ══════════════════════════════════════════════════
// 4. Multi-Source Market Search
// ══════════════════════════════════════════════════

/// A skill found in an external market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSkill {
    pub source: String,       // "github" | "anthropic" | "awesome-claude-skills"
    pub name: String,
    pub description: String,
    pub url: String,
    pub author: String,
    pub stars: Option<u32>,
    pub downloaded: bool,
}

/// Search for skills across multiple sources
pub async fn search_market(query: &str) -> Result<Vec<MarketSkill>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();

    // Source 1: GitHub code search for SKILL.md
    let github_query = format!("{} filename:SKILL.md", query);
    if let Ok(res) = client
        .get("https://api.github.com/search/code")
        .header("User-Agent", "OMNIX-DevFlow")
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[("q", &github_query), ("per_page", &"10".to_string())])
        .send()
        .await
    {
        if let Ok(body) = res.json::<serde_json::Value>().await {
            if let Some(items) = body["items"].as_array() {
                for item in items {
                    let name = item["name"].as_str().unwrap_or("unknown").to_string();
                    let repo = item["repository"]["full_name"].as_str().unwrap_or("").to_string();
                    let url = item["html_url"].as_str().unwrap_or("").to_string();
                    results.push(MarketSkill {
                        source: "github".into(),
                        name: name.replace(".md", ""),
                        description: format!("From {}", repo),
                        url,
                        author: repo.split('/').next().unwrap_or("").into(),
                        stars: item["repository"]["stargazers_count"].as_u64().map(|v| v as u32),
                        downloaded: false,
                    });
                }
            }
        }
    }

    // Source 2: Anthropic official skills
    if let Ok(res) = client
        .get("https://api.github.com/search/code")
        .header("User-Agent", "OMNIX-DevFlow")
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[("q", &format!("repo:anthropics/skills {} filename:SKILL.md", query)), ("per_page", &"5".to_string())])
        .send()
        .await
    {
        if let Ok(body) = res.json::<serde_json::Value>().await {
            if let Some(items) = body["items"].as_array() {
                for item in items {
                    let name = item["name"].as_str().unwrap_or("unknown").to_string();
                    let url = item["html_url"].as_str().unwrap_or("").to_string();
                    results.push(MarketSkill {
                        source: "anthropic".into(),
                        name: name.replace(".md", ""),
                        description: "Anthropic official skill".into(),
                        url,
                        author: "anthropics".into(),
                        stars: None,
                        downloaded: false,
                    });
                }
            }
        }
    }

    // Source 3: awesome-claude-skills
    if let Ok(res) = client
        .get("https://api.github.com/search/code")
        .header("User-Agent", "OMNIX-DevFlow")
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[("q", &format!("repo:ComposioHQ/awesome-claude-skills {} filename:SKILL.md", query)), ("per_page", &"5".to_string())])
        .send()
        .await
    {
        if let Ok(body) = res.json::<serde_json::Value>().await {
            if let Some(items) = body["items"].as_array() {
                for item in items {
                    let name = item["name"].as_str().unwrap_or("unknown").to_string();
                    let url = item["html_url"].as_str().unwrap_or("").to_string();
                    results.push(MarketSkill {
                        source: "awesome-claude-skills".into(),
                        name: name.replace(".md", ""),
                        description: "Community curated skill".into(),
                        url,
                        author: "ComposioHQ".into(),
                        stars: None,
                        downloaded: false,
                    });
                }
            }
        }
    }

    Ok(results)
}

// ══════════════════════════════════════════════════
// 5. Experience Distillation
// ══════════════════════════════════════════════════

/// A skill recommendation from project history analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillRecommendation {
    pub suggested_name: String,
    pub suggested_category: String,
    pub reason: String,
    pub source_evidence: Vec<String>,
    pub confidence: f32,
}

/// Analyze a project directory and recommend skills to create.
/// Examines: Git log, logs/ directory, package.json/Cargo.toml dependencies.
pub fn distill_from_project(project_path: &str) -> Result<Vec<DistillRecommendation>, String> {
    let root = PathBuf::from(project_path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", project_path));
    }

    let mut recommendations = Vec::new();
    let mut evidence = Vec::new();

    // 1. Analyze dependencies
    let deps = extract_dependencies(&root);
    if !deps.is_empty() {
        evidence.push(format!("Dependencies: {}", deps.join(", ")));

        // Recommend skills based on dependencies
        let dep_str = deps.join(" ").to_lowercase();
        if dep_str.contains("react") || dep_str.contains("vue") || dep_str.contains("svelte") {
            recommendations.push(DistillRecommendation {
                suggested_name: "frontend-development".into(),
                suggested_category: "研发效能".into(),
                reason: "Project uses frontend framework — a frontend development skill would help".into(),
                source_evidence: vec![format!("Found frontend deps: {}", deps.iter().filter(|d| d.contains("react") || d.contains("vue")).cloned().collect::<Vec<_>>().join(", "))],
                confidence: 0.7,
            });
        }
        if dep_str.contains("tokio") || dep_str.contains("async-std") {
            recommendations.push(DistillRecommendation {
                suggested_name: "async-rust".into(),
                suggested_category: "研发效能".into(),
                reason: "Project uses async runtime — an async Rust skill would help avoid common pitfalls".into(),
                source_evidence: vec!["Found async runtime dependency".into()],
                confidence: 0.8,
            });
        }
        if dep_str.contains("sqlx") || dep_str.contains("diesel") || dep_str.contains("rusqlite") || dep_str.contains("sea-orm") {
            recommendations.push(DistillRecommendation {
                suggested_name: "database-patterns".into(),
                suggested_category: "数据".into(),
                reason: "Project uses database — a database patterns skill would help with migrations and queries".into(),
                source_evidence: vec!["Found database dependency".into()],
                confidence: 0.7,
            });
        }
    }

    // 2. Analyze Git log for common patterns
    if let Ok(output) = std::process::Command::new("git")
        .arg("-C").arg(&root)
        .arg("log").arg("--oneline").arg("-50")
        .output()
    {
        let log = String::from_utf8_lossy(&output.stdout);
        let log_lower = log.to_lowercase();

        let fix_count = log_lower.matches("fix").count();
        let _feat_count = log_lower.matches("feat").count();
        let refactor_count = log_lower.matches("refactor").count();

        if fix_count > 5 {
            evidence.push(format!("Git log: {} fix commits in last 50", fix_count));
            recommendations.push(DistillRecommendation {
                suggested_name: "debugging-workflow".into(),
                suggested_category: "调试诊断".into(),
                reason: format!("High fix commit count ({}) suggests a debugging skill would be valuable", fix_count),
                source_evidence: vec![format!("{} fix commits found", fix_count)],
                confidence: 0.6,
            });
        }

        if refactor_count > 3 {
            evidence.push(format!("Git log: {} refactor commits in last 50", refactor_count));
        }
    }

    // 3. Check for existing logs/ directory
    let logs_dir = root.join("logs");
    if logs_dir.exists() {
        let log_files = count_files_recursive(&logs_dir);
        evidence.push(format!("Found logs/ directory with {} files", log_files));

        if log_files > 10 {
            recommendations.push(DistillRecommendation {
                suggested_name: "development-logging".into(),
                suggested_category: "研发效能".into(),
                reason: "Project has extensive logs — a logging skill could standardize the practice".into(),
                source_evidence: vec![format!("{} log files found", log_files)],
                confidence: 0.5,
            });
        }
    }

    // 4. Check for test patterns
    let has_tests = root.join("tests").exists()
        || root.join("test").exists()
        || root.join("__tests__").exists()
        || root.join("src").join("tests").exists();

    if has_tests {
        evidence.push("Test directory found".into());
    }

    Ok(recommendations)
}

/// Extract dependencies from package.json or Cargo.toml
fn extract_dependencies(root: &PathBuf) -> Vec<String> {
    let mut deps = Vec::new();

    // package.json
    let pkg = root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            for section in &["dependencies", "devDependencies"] {
                if let Some(obj) = json[section].as_object() {
                    deps.extend(obj.keys().cloned());
                }
            }
        }
    }

    // Cargo.toml
    let cargo = root.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&cargo) {
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') && !trimmed.starts_with('[') && trimmed.contains('=') {
                if let Some(name) = trimmed.split('=').next() {
                    let name = name.trim().to_string();
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                        deps.push(name);
                    }
                }
            }
        }
    }

    deps
}

/// Count files recursively in a directory
fn count_files_recursive(dir: &PathBuf) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}
