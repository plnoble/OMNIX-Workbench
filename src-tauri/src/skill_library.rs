//! Skill Library Features
//!
//! 1. Semantic Skill Auto-Injection — match skills to user messages
//! 2. Dual-Agent Sandbox Testing — adversarial skill quality testing
//! 3. Text Protocol Interception — parse AI output for skill:/memory:/task: blocks
//! 4. Multi-Source Market Search — search GitHub/Anthropic for skills
//! 5. Experience Distillation — extract skills from project history

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::db::DbManager;
use crate::proc::NoWindow;

// ══════════════════════════════════════════════════
// 1. Semantic Skill Auto-Injection
// ══════════════════════════════════════════════════

/// 「消息里出现这个词 → 给这个分类的技能加分」。
///
/// 原本只有 ASCII 词（bug / fix / deploy …），判断又是直接
/// `message.contains(keyword)`——纯中文消息一个都不含，**整条加权通道对中文是关着
/// 的**。这跟同一函数里的分词坑是两处独立的问题：分词修好了只是让描述和名字能匹配
/// 上，分类加权还得靠这张表里有中文词才会动。
///
/// 每个分类给一组中文触发词，都用双字词——单字（「写」「改」）太泛，会把不相干的
/// 消息也抬过门槛。
///
/// 提到模块级常量：它原本写在「遍历每个技能」的循环体里，每条技能都要重新分配一次
/// 这个 Vec。
const CATEGORY_BOOSTS: &[(&str, &str)] = &[
    ("bug", "调试诊断"),
    ("error", "调试诊断"),
    ("fix", "调试诊断"),
    ("报错", "调试诊断"),
    ("调试", "调试诊断"),
    ("崩溃", "调试诊断"),
    ("异常", "调试诊断"),
    ("修复", "调试诊断"),
    ("code", "研发效能"),
    ("review", "研发效能"),
    ("test", "研发效能"),
    ("代码", "研发效能"),
    ("审查", "研发效能"),
    ("测试", "研发效能"),
    ("重构", "研发效能"),
    ("write", "文档办公"),
    ("doc", "文档办公"),
    ("translate", "文档办公"),
    ("文档", "文档办公"),
    ("翻译", "文档办公"),
    ("撰写", "文档办公"),
    ("security", "安全"),
    ("安全", "安全"),
    ("漏洞", "安全"),
    ("deploy", "部署"),
    ("部署", "部署"),
    ("发布", "部署"),
    ("上线", "部署"),
    ("git", "版本控制"),
    ("提交", "版本控制"),
    ("分支", "版本控制"),
    ("design", "设计"),
    ("ui", "设计"),
    ("设计", "设计"),
    ("界面", "设计"),
    ("api", "接口"),
    ("接口", "接口"),
];

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
/// With `official_only`, restricts to the 正式池 (pool = 'official') — the pool
/// gate for gateway auto-injection: pending skills are NEVER injected.
pub fn match_skills_for_message(
    db: &DbManager,
    message: &str,
    official_only: bool,
) -> Vec<SkillMatch> {
    let conn = match db.get_connection() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let sql = if official_only {
        "SELECT name, description, category, file_path FROM skills WHERE is_active = 1 AND pool = 'official'"
    } else {
        "SELECT name, description, category, file_path FROM skills WHERE is_active = 1"
    };
    let mut stmt = match conn.prepare(sql) {
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
    // 中文没有空格：`split_whitespace` 会把一整句话切成**一个** token，于是下面
    // 每一处 `contains(word)` 都变成「整句话得是描述的子串」——永远不成立。结果
    // 是纯中文消息只剩「技能名恰好是消息子串」那一条路能给分，其余全是死码。
    //
    // 复用知识库那套 CJK 二元切分（`segment_for_index`，已在 BM25 索引/查询两端
    // 用了很久）：「看这段代码」→「看这 这段 段代 代码 …」。描述那边不用切，
    // `contains` 本来就是子串匹配，二元组能直接命中。英文原样保留。
    let segmented = crate::knowledge::segment_for_index(&message_lower);
    let message_words: Vec<&str> = segmented.split_whitespace().collect();
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
            // 旧写法 `word.len() < 3` 按**字节**算：任何中文 token 都 ≥3 字节，
            // 等于对中文完全不设防；而切成二元组后又会把每个二元组都放行。
            // 改按字符数，并且只对 ASCII 保留「跳过 1~2 个字母的虚词」这层意图。
            let char_count = word.chars().count();
            if char_count < 2 || (word.is_ascii() && char_count < 3) {
                continue;
            }

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

        for (keyword, boost_cat) in CATEGORY_BOOSTS {
            if message_lower.contains(keyword)
                && (cat_lower.contains(boost_cat) || desc_lower.contains(keyword))
            {
                score += 1.5;
            }
        }

        if score >= 3.0 {
            // Read first 200 chars of skill content for preview
            let preview = std::fs::read_to_string(
                PathBuf::from(&file_path).join(format!("{}_core.md", name)),
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

    let conn = match db.get_connection() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut injection = String::from("\n\n<auto_injected_skills>\nThe following skills are automatically activated based on the current task:\n\n");

    for m in matches {
        // Read full skill content — prefer the central store, fall back from the
        // profile file to plain SKILL.md so imported skills always inject.
        let content: String = conn
            .query_row(
                "SELECT CASE WHEN central_path != '' THEN central_path ELSE file_path END
                 FROM skills WHERE name = ?1",
                rusqlite::params![m.skill_name],
                |r| r.get(0),
            )
            .ok()
            .and_then(|fp: String| {
                let dir = PathBuf::from(&fp);
                std::fs::read_to_string(dir.join(format!("{}_core.md", m.skill_name)))
                    .or_else(|_| std::fs::read_to_string(dir.join("SKILL.md")))
                    .ok()
            })
            .unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        // Cap each skill so a bloated skill can't blow up the request.
        let capped: String = content.chars().take(6000).collect();
        injection.push_str(&format!(
            "## Skill: {} (relevance: {:.1})\n{}\n\n",
            m.skill_name, m.relevance_score, capped
        ));
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
        input: format!(
            "Using the {} skill, explain your approach to a common task in your domain.",
            skill_name
        ),
        expected_behavior: "Should demonstrate the skill's core knowledge and workflow".into(),
    });

    // Test 2: Edge case
    cases.push(SandboxTestCase {
        input: format!(
            "Apply {} to handle an edge case or error scenario.",
            skill_name
        ),
        expected_behavior: "Should identify the edge case and provide a structured solution".into(),
    });

    // Test 3: Anti-pattern detection
    if skill_content.contains("Anti-Pattern")
        || skill_content.contains("anti-pattern")
        || skill_content.contains("Do NOT")
    {
        cases.push(SandboxTestCase {
            input: format!(
                "What should you NOT do when using the {} skill?",
                skill_name
            ),
            expected_behavior: "Should correctly list the anti-patterns from the skill".into(),
        });
    }

    cases
}

// ══════════════════════════════════════════════════
// 3. Text Protocol Interception
// ══════════════════════════════════════════════════

/// Protocol action detected in AI output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolAction {
    pub action_type: String, // "skill" | "memory" | "task" | "config"
    pub target: String,      // skill ID, memory key, task title, etc.
    pub content: String,     // payload
    pub raw_block: String,   // original code block
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
        if let Some(stripped) = line.strip_prefix("```") {
            let lang_tag = stripped.trim();

            // Check for protocol tags
            if lang_tag.starts_with("skill:")
                || lang_tag.starts_with("memory:")
                || lang_tag.starts_with("task:")
                || lang_tag.starts_with("config:")
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
pub fn execute_protocol_action(action: &ProtocolAction, db: &DbManager) -> Result<String, String> {
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
    pub source: String, // "github" | "anthropic" | "awesome-claude-skills"
    pub name: String,
    pub description: String,
    pub url: String,
    pub author: String,
    pub stars: Option<u32>,
    pub downloaded: bool,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub content_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSkillPreview {
    pub skill: MarketSkill,
    pub content: String,
    pub content_hash: String,
}

pub fn github_raw_url(html_url: &str) -> Option<String> {
    let marker = "/blob/";
    let index = html_url.find(marker)?;
    let (repo, rest) = html_url.split_at(index);
    Some(format!(
        "https://raw.githubusercontent.com/{}/{}",
        repo.trim_start_matches("https://github.com/"),
        rest.trim_start_matches(marker)
    ))
}

pub async fn fetch_market_skill(skill: &MarketSkill) -> Result<MarketSkillPreview, String> {
    let raw_url = github_raw_url(&skill.url)
        .ok_or_else(|| "市场条目不是可预览的 GitHub SKILL.md 地址".to_string())?;
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?
        .get(raw_url)
        .header("User-Agent", "OMNIX-Workbench")
        .send()
        .await
        .map_err(|e| format!("下载 SKILL.md 失败: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("下载 SKILL.md 失败: HTTP {}", response.status()));
    }
    let content = response.text().await.map_err(|e| e.to_string())?;
    if content.trim().is_empty() || !content.contains('#') {
        return Err("远程 SKILL.md 内容为空或格式无效".into());
    }
    Ok(MarketSkillPreview {
        skill: skill.clone(),
        content_hash: crate::hash::fnv1a_hash(&content),
        content,
    })
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
        .header("User-Agent", "OMNIX-Workbench")
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[("q", &github_query), ("per_page", &"10".to_string())])
        .send()
        .await
    {
        if let Ok(body) = res.json::<serde_json::Value>().await {
            if let Some(items) = body["items"].as_array() {
                for item in items {
                    let path = item["path"].as_str().unwrap_or("SKILL.md").to_string();
                    let name = std::path::Path::new(&path)
                        .parent()
                        .and_then(|parent| parent.file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or("skill")
                        .to_string();
                    let repo = item["repository"]["full_name"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let repo_url = item["repository"]["html_url"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let url = item["html_url"].as_str().unwrap_or("").to_string();
                    results.push(MarketSkill {
                        source: "github".into(),
                        name,
                        description: format!("From {}", repo),
                        url,
                        author: repo.split('/').next().unwrap_or("").into(),
                        stars: item["repository"]["stargazers_count"]
                            .as_u64()
                            .map(|v| v as u32),
                        downloaded: false,
                        repo_url,
                        revision: item["repository"]["default_branch"]
                            .as_str()
                            .unwrap_or("main")
                            .into(),
                        path,
                        content_sha: item["sha"].as_str().unwrap_or("").into(),
                    });
                }
            }
        }
    }

    // Source 2: Anthropic official skills
    if let Ok(res) = client
        .get("https://api.github.com/search/code")
        .header("User-Agent", "OMNIX-Workbench")
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[
            (
                "q",
                &format!("repo:anthropics/skills {} filename:SKILL.md", query),
            ),
            ("per_page", &"5".to_string()),
        ])
        .send()
        .await
    {
        if let Ok(body) = res.json::<serde_json::Value>().await {
            if let Some(items) = body["items"].as_array() {
                for item in items {
                    let path = item["path"].as_str().unwrap_or("SKILL.md").to_string();
                    let name = std::path::Path::new(&path)
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("skill")
                        .to_string();
                    let url = item["html_url"].as_str().unwrap_or("").to_string();
                    results.push(MarketSkill {
                        source: "anthropic".into(),
                        name,
                        description: "Anthropic official skill".into(),
                        url,
                        author: "anthropics".into(),
                        stars: None,
                        downloaded: false,
                        repo_url: item["repository"]["html_url"]
                            .as_str()
                            .unwrap_or("https://github.com/anthropics/skills")
                            .into(),
                        revision: item["repository"]["default_branch"]
                            .as_str()
                            .unwrap_or("main")
                            .into(),
                        path,
                        content_sha: item["sha"].as_str().unwrap_or("").into(),
                    });
                }
            }
        }
    }

    // Source 3: awesome-claude-skills
    if let Ok(res) = client
        .get("https://api.github.com/search/code")
        .header("User-Agent", "OMNIX-Workbench")
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[
            (
                "q",
                &format!(
                    "repo:ComposioHQ/awesome-claude-skills {} filename:SKILL.md",
                    query
                ),
            ),
            ("per_page", &"5".to_string()),
        ])
        .send()
        .await
    {
        if let Ok(body) = res.json::<serde_json::Value>().await {
            if let Some(items) = body["items"].as_array() {
                for item in items {
                    let path = item["path"].as_str().unwrap_or("SKILL.md").to_string();
                    let name = std::path::Path::new(&path)
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("skill")
                        .to_string();
                    let url = item["html_url"].as_str().unwrap_or("").to_string();
                    results.push(MarketSkill {
                        source: "awesome-claude-skills".into(),
                        name,
                        description: "Community curated skill".into(),
                        url,
                        author: "ComposioHQ".into(),
                        stars: None,
                        downloaded: false,
                        repo_url: item["repository"]["html_url"]
                            .as_str()
                            .unwrap_or("https://github.com/ComposioHQ/awesome-claude-skills")
                            .into(),
                        revision: item["repository"]["default_branch"]
                            .as_str()
                            .unwrap_or("main")
                            .into(),
                        path,
                        content_sha: item["sha"].as_str().unwrap_or("").into(),
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
                reason: "Project uses frontend framework — a frontend development skill would help"
                    .into(),
                source_evidence: vec![format!(
                    "Found frontend deps: {}",
                    deps.iter()
                        .filter(|d| d.contains("react") || d.contains("vue"))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )],
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
        if dep_str.contains("sqlx")
            || dep_str.contains("diesel")
            || dep_str.contains("rusqlite")
            || dep_str.contains("sea-orm")
        {
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
        .arg("-C")
        .arg(&root)
        .arg("log")
        .arg("--oneline")
        .arg("-50")
        .no_window()
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
                reason: format!(
                    "High fix commit count ({}) suggests a debugging skill would be valuable",
                    fix_count
                ),
                source_evidence: vec![format!("{} fix commits found", fix_count)],
                confidence: 0.6,
            });
        }

        if refactor_count > 3 {
            evidence.push(format!(
                "Git log: {} refactor commits in last 50",
                refactor_count
            ));
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
                reason:
                    "Project has extensive logs — a logging skill could standardize the practice"
                        .into(),
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
fn extract_dependencies(root: &Path) -> Vec<String> {
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
                    if !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    {
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

#[cfg(test)]
mod injection_matching_tests {
    use super::*;
    use crate::db::DbManager;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_skillmatch_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    fn add_skill(db: &DbManager, name: &str, description: &str, category: &str) {
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO skills (name, description, file_path, category, pool, is_active)
             VALUES (?1, ?2, '', ?3, 'official', 1)",
            rusqlite::params![name, description, category],
        )
        .unwrap();
    }

    #[test]
    fn a_chinese_message_matches_a_chinese_skill() {
        let db = test_db("zh");
        add_skill(&db, "代码审查", "审查代码质量，找出潜在缺陷和风格问题", "研发效能");
        let hits = match_skills_for_message(&db, "帮我看看这段代码写得怎么样", true);
        assert!(
            !hits.is_empty(),
            "中文消息没能命中语义完全对口的中文技能——命中数 {}",
            hits.len()
        );
    }

    #[test]
    fn an_english_message_matches_an_english_skill() {
        let db = test_db("en");
        add_skill(&db, "code-review", "review code quality and find defects", "研发效能");
        let hits = match_skills_for_message(&db, "please review the quality of this code", true);
        assert!(!hits.is_empty(), "英文消息没能命中对口的英文技能");
    }

    /// 分类加权对中文也得管用。
    ///
    /// 这里技能名「灰度放量」和描述「按批次逐步扩大流量比例」跟消息**没有任何字面
    /// 重叠**——分词修好了也一分拿不到。唯一的得分来源是分类加权：「上线」和
    /// 「发布」各 +1.5，正好凑到 3.0 的门槛。所以这条测的就是 CATEGORY_BOOSTS
    /// 里有没有中文词，把中文条目删掉它必红。
    #[test]
    fn chinese_category_boost_can_carry_a_match_on_its_own() {
        let db = test_db("zh_boost");
        add_skill(&db, "灰度放量", "按批次逐步扩大流量比例", "部署");
        let hits = match_skills_for_message(&db, "准备上线，先发布到预发环境", true);
        assert!(
            !hits.is_empty(),
            "中文分类加权没生效——「上线」「发布」都该给「部署」类技能加分"
        );
    }

    /// 中英混写时，「把」「的」「和」这类单字会被夹在英文 token 之间单独切出来。
    ///
    /// 旧的 `word.len() < 3` 按**字节**算，单个汉字正好 3 字节——一个都拦不住。
    /// 而「的」「和」几乎出现在每一条中文描述里，两个虚词就凑够 4.0 分越过 3.0
    /// 的门槛，于是任何中英混写的消息都能命中任何技能。改按字符数才拦得住。
    #[test]
    fn isolated_single_chinese_characters_do_not_carry_a_match() {
        let db = test_db("zh_single");
        add_skill(&db, "代码审查", "审查代码的质量，找出潜在缺陷和风格问题", "研发效能");
        // 「把 config.json 里的 port 和 host 改一下」——跟代码审查毫无关系。
        let hits = match_skills_for_message(&db, "把 config.json 的 port 和 host 改一下", true);
        assert!(
            hits.is_empty(),
            "「的」「和」这类单字不该独自撑起一次命中：{:?}",
            hits.iter().map(|h| (&h.skill_name, h.relevance_score)).collect::<Vec<_>>()
        );
    }

    /// 二元切分会让 token 数量涨好几倍，光证明「能召回」不够——还得证明没有
    /// 松到什么都召回。不然修完就是从「一个都不中」滑到「个个都中」，同样没用。
    #[test]
    fn an_unrelated_chinese_message_does_not_match() {
        let db = test_db("zh_neg");
        add_skill(&db, "代码审查", "审查代码质量，找出潜在缺陷和风格问题", "研发效能");
        let hits = match_skills_for_message(&db, "明天北京的天气怎么样", true);
        assert!(
            hits.is_empty(),
            "不相干的中文消息不该命中：{:?}",
            hits.iter().map(|h| (&h.skill_name, h.relevance_score)).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod injection_ranking_corpus {
    use super::*;
    use crate::db::DbManager;

    /// 一份**互相竞争**的技能样本。
    ///
    /// 上面那组测试每条只往库里放一个技能，于是「命中」是必然的——测不出排名。
    /// 而网关只注入前 2 名（`proxy_anthropic.rs` 的 `truncate(2)`），所以「匹配上了
    /// 但排第 3」对用户来说和没匹配上完全一样。要守排名就必须有陪跑的。
    const FIXTURE: &[(&str, &str, &str)] = &[
        ("代码审查", "审查代码质量，找出潜在缺陷和风格问题", "研发效能"),
        ("单元测试", "为函数补充单元测试，覆盖边界情况", "研发效能"),
        ("崩溃排查", "分析报错堆栈，定位崩溃的根本原因", "调试诊断"),
        ("性能优化", "找出热点函数，降低耗时和内存占用", "研发效能"),
        ("接口设计", "设计接口的路径、参数和返回结构", "接口"),
        ("灰度发布", "按批次逐步扩大流量，准备回滚方案", "部署"),
        ("文档撰写", "把功能说明写成用户能看懂的文档", "文档办公"),
        ("中英翻译", "在中文和英文之间翻译技术内容", "文档办公"),
    ];

    /// 考题：一句话 → 谁必须进前 2、谁绝不能出现。
    ///
    /// 断言的是**前 2 名**而不是第 1 名，因为那才是网关真正会注入的范围；第 1 名
    /// 在几个近义技能之间谁高谁低，属于可以接受的浮动。
    ///
    /// 注意这里只考「字面上够得着」的题。这个匹配器纯粹是词法的，「页面加载太慢」
    /// 找不到「性能优化」——那需要语义理解，是它不具备的能力。把不具备的能力写成
    /// 考题，只会得到一条永远红的测试，而不是一个更好的匹配器。
    const CASES: &[(&str, &str, &[&str])] = &[
        ("帮我审查一下这段代码", "代码审查", &["中英翻译", "灰度发布"]),
        ("这个函数崩溃了，帮我看看报错堆栈", "崩溃排查", &["文档撰写", "中英翻译"]),
        ("给支付模块补几个单元测试", "单元测试", &["灰度发布", "中英翻译"]),
        ("准备上线，先发布到预发环境", "灰度发布", &["中英翻译", "单元测试"]),
        ("把这段技术内容翻译成英文", "中英翻译", &["崩溃排查", "灰度发布"]),
        ("设计一下这个接口的参数和返回结构", "接口设计", &["中英翻译", "崩溃排查"]),
        ("写一份用户能看懂的功能说明文档", "文档撰写", &["崩溃排查", "灰度发布"]),
        // 下面两条**只**靠描述匹配得分：消息里既没有技能名、也没有任何
        // CATEGORY_BOOSTS 里的词。加权走的是原始消息 `contains`，不经过分词，
        // 所以上面那些题即使分词坏掉也照样能过——这两条才真正守住分词那条路径。
        ("找出耗时最多的那几个函数", "性能优化", &["中英翻译", "灰度发布"]),
        ("覆盖一下边界情况", "单元测试", &["中英翻译", "崩溃排查"]),
    ];

    fn corpus_db() -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_skillcorpus_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let db = DbManager::new_with_path(path);
        {
            let conn = db.get_connection().unwrap();
            for (name, desc, cat) in FIXTURE {
                conn.execute(
                    "INSERT INTO skills (name, description, file_path, category, pool, is_active)
                     VALUES (?1, ?2, '', ?3, 'official', 1)",
                    rusqlite::params![name, desc, cat],
                )
                .unwrap();
            }
        }
        db
    }

    /// 跑完整张表，**把所有不合格的一次列出来**。
    ///
    /// 逐条 `assert!` 会在第一条就停，于是调打分权重时只能看见第一个坏掉的，看不见
    /// 「修好 5 条、弄坏 1 条」的全貌。这是这套东西唯一值得从 evaluator 那里学的
    /// 性质：要看总账，不是看第一笔。
    #[test]
    fn the_ranking_corpus_holds() {
        let db = corpus_db();
        let mut failures: Vec<String> = Vec::new();

        for (message, expected, forbidden) in CASES {
            let hits = match_skills_for_message(&db, message, true);
            let ranked: Vec<&str> = hits.iter().map(|h| h.skill_name.as_str()).collect();
            // 网关注入前 2 名，所以排在第 3 及之后等于没命中。
            let top2 = &ranked[..ranked.len().min(2)];

            if !top2.contains(expected) {
                failures.push(format!(
                    "「{message}」→ 期待「{expected}」进前 2，实际排名 {ranked:?}"
                ));
            }
            for bad in *forbidden {
                if ranked.contains(bad) {
                    failures.push(format!("「{message}」→ 不该出现「{bad}」，实际排名 {ranked:?}"));
                }
            }
        }

        assert!(
            failures.is_empty(),
            "考题表 {} / {} 条不合格：\n{}",
            failures.len(),
            CASES.len(),
            failures.join("\n")
        );
    }
}
