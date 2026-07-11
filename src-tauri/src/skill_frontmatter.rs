//! Skill Frontmatter — SKILL.md YAML frontmatter parsing.
//!
//! Provides YAML frontmatter parsing and generation for SKILL.md files.
//! Frontmatter makes skills self-describing: name, description, category,
//! version, author, source, and argument-hint are embedded in the file itself.

use serde::{Deserialize, Serialize};

/// YAML frontmatter metadata embedded in SKILL.md files
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillFrontmatter {
    /// Skill name (must match directory name)
    pub name: Option<String>,
    /// One-line description
    pub description: Option<String>,
    /// Category tag (e.g., "Engineering", "Writing")
    pub category: Option<String>,
    /// Semantic version (e.g., "1.0.0")
    pub version: Option<String>,
    /// Author name or org
    pub author: Option<String>,
    /// Source URL (Git repo, package registry, etc.)
    pub source: Option<String>,
    /// Usage hint for arguments (e.g., "<file-or-pattern>")
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<String>,
    /// Dependent skill names
    pub skills: Option<Vec<String>>,
}

/// Parse YAML frontmatter from SKILL.md content.
///
/// Frontmatter is delimited by `---` at the start and end:
/// ```markdown
/// ---
/// name: my-skill
/// description: Does something useful
/// category: Engineering
/// version: "1.0.0"
/// ---
/// # My Skill
/// Actual content here...
/// ```
///
/// Returns (frontmatter, body) where body is everything after the closing `---`.
/// If no frontmatter is found, returns (Default, full_content).
pub fn parse_frontmatter(content: &str) -> (SkillFrontmatter, String) {
    let trimmed = content.trim_start();

    // Must start with ---
    if !trimmed.starts_with("---") {
        return (SkillFrontmatter::default(), content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let closing_pos = match after_first.find("\n---") {
        Some(pos) => pos,
        None => return (SkillFrontmatter::default(), content.to_string()),
    };

    let yaml_str = &after_first[..closing_pos];
    let body_start = closing_pos + 4; // skip "\n---"
    let body = after_first[body_start..].trim_start_matches('\n').to_string();

    // Parse YAML — use simple line-by-line parsing to avoid serde_yaml dependency
    let frontmatter = parse_yaml_simple(yaml_str);

    (frontmatter, body)
}

/// Generate SKILL.md content with frontmatter prepended.
///
/// If the body already has frontmatter, it will be replaced.
pub fn generate_with_frontmatter(frontmatter: &SkillFrontmatter, body: &str) -> String {
    let yaml = generate_yaml(frontmatter);
    if yaml.is_empty() {
        return body.to_string();
    }
    format!("---\n{}\n---\n\n{}", yaml, body)
}

/// Simple YAML parser for frontmatter fields (avoids serde_yaml dependency).
///
/// Handles flat key-value pairs and simple arrays:
/// ```yaml
/// name: my-skill
/// description: "A useful skill"
/// skills:
///   - skill-a
///   - skill-b
/// ```
fn parse_yaml_simple(yaml: &str) -> SkillFrontmatter {
    let mut fm = SkillFrontmatter::default();
    let mut current_key: Option<String> = None;
    let mut array_values: Vec<String> = Vec::new();

    for line in yaml.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Array item: starts with "- "
        if trimmed.starts_with("- ") {
            let val = trimmed[2..].trim().trim_matches('"').to_string();
            array_values.push(val);
            continue;
        }

        // If we were collecting array values, flush them
        if !array_values.is_empty() {
            if let Some(ref key) = current_key {
                set_field(&mut fm, key, &array_values.join(","));
            }
            array_values.clear();
        }

        // Key-value pair: "key: value"
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let val_raw = trimmed[colon_pos + 1..].trim();

            // Store previous key if any
            current_key = Some(key.clone());

            if val_raw.is_empty() {
                // Value is on next lines (array)
                continue;
            }

            let val = val_raw.trim_matches('"').to_string();
            set_field(&mut fm, &key, &val);
        }
    }

    // Flush any remaining array values
    if !array_values.is_empty() {
        if let Some(ref key) = current_key {
            set_field(&mut fm, key, &array_values.join(","));
        }
    }

    fm
}

/// Set a field on SkillFrontmatter by key name
fn set_field(fm: &mut SkillFrontmatter, key: &str, value: &str) {
    match key {
        "name" => fm.name = Some(value.to_string()),
        "description" => fm.description = Some(value.to_string()),
        "category" => fm.category = Some(value.to_string()),
        "version" => fm.version = Some(value.to_string()),
        "author" => fm.author = Some(value.to_string()),
        "source" => fm.source = Some(value.to_string()),
        "argument-hint" => fm.argument_hint = Some(value.to_string()),
        "skills" => {
            // Parse comma-separated or single value
            let skills: Vec<String> = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !skills.is_empty() {
                fm.skills = Some(skills);
            }
        }
        _ => {} // Ignore unknown fields
    }
}

/// Generate simple YAML from SkillFrontmatter
fn generate_yaml(fm: &SkillFrontmatter) -> String {
    let mut lines = Vec::new();

    if let Some(ref v) = fm.name {
        lines.push(format!("name: {}", v));
    }
    if let Some(ref v) = fm.description {
        // Quote if contains special chars
        if v.contains(':') || v.contains('#') || v.contains('"') {
            lines.push(format!("description: \"{}\"", v.replace('"', "\\\"")));
        } else {
            lines.push(format!("description: {}", v));
        }
    }
    if let Some(ref v) = fm.category {
        lines.push(format!("category: {}", v));
    }
    if let Some(ref v) = fm.version {
        lines.push(format!("version: \"{}\"", v));
    }
    if let Some(ref v) = fm.author {
        lines.push(format!("author: {}", v));
    }
    if let Some(ref v) = fm.source {
        lines.push(format!("source: {}", v));
    }
    if let Some(ref v) = fm.argument_hint {
        lines.push(format!("argument-hint: {}", v));
    }
    if let Some(ref skills) = fm.skills {
        if !skills.is_empty() {
            lines.push("skills:".to_string());
            for s in skills {
                lines.push(format!("  - {}", s));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: web-design-guidelines
description: Review UI code for compliance
category: Design
version: "1.0.0"
author: vercel
argument-hint: <file-or-pattern>
skills:
  - code-reviewer
  - frontend-builder
---

# Web Design Guidelines

Actual content here."#;

        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.name, Some("web-design-guidelines".into()));
        assert_eq!(fm.description, Some("Review UI code for compliance".into()));
        assert_eq!(fm.category, Some("Design".into()));
        assert_eq!(fm.version, Some("1.0.0".into()));
        assert_eq!(fm.author, Some("vercel".into()));
        assert_eq!(fm.argument_hint, Some("<file-or-pattern>".into()));
        assert!(body.starts_with("# Web Design Guidelines"));
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "# Just a heading\n\nSome content.";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.name, None);
        assert_eq!(body, content);
    }

    #[test]
    fn test_generate_roundtrip() {
        let fm = SkillFrontmatter {
            name: Some("test-skill".into()),
            description: Some("A test skill".into()),
            category: Some("Engineering".into()),
            version: Some("1.0.0".into()),
            ..Default::default()
        };
        let body = "# Test Skill\n\nContent here.";
        let generated = generate_with_frontmatter(&fm, body);

        let (parsed_fm, parsed_body) = parse_frontmatter(&generated);
        assert_eq!(parsed_fm.name, fm.name);
        assert_eq!(parsed_fm.description, fm.description);
        assert_eq!(parsed_fm.category, fm.category);
        assert!(parsed_body.starts_with("# Test Skill"));
    }
}
