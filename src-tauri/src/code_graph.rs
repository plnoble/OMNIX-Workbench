//! Code Graph — Architecture Knowledge Graph (Understand-Anything inspired)
//!
//! Builds a structured JSON graph of a project's architecture:
//! files, functions, classes, dependencies, layers, clusters.
//! Supports incremental updates via structural fingerprinting,
//! .omnixignore filtering, and LLM output validation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ══════════════════════════════════════════════════
// Types
// ══════════════════════════════════════════════════

/// Node types in the architecture graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    File,
    Directory,
    Module,
    Function,
    Class,
    Interface,
    Component,
    Hook,
    Route,
    Config,
    Test,
    Style,
    Asset,
    Domain,
    Flow,
    External,
}

/// Edge types in the architecture graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Contains,
    Imports,
    Exports,
    Calls,
    Extends,
    Implements,
    DependsOn,
    BelongsTo,
    Configures,
    Tests,
    Styles,
}

/// Architecture layer classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArchLayer {
    Api,
    Service,
    Data,
    Ui,
    Utility,
    Config,
    Test,
    Infrastructure,
    Unknown,
}

/// A node in the architecture graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub node_type: NodeType,
    pub path: String,
    pub layer: ArchLayer,
    pub language: Option<String>,
    pub summary: Option<String>,
    pub line_count: u32,
    pub fingerprint: String,
    pub complexity: Option<String>,
    pub tags: Vec<String>,
}

/// An edge in the architecture graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub weight: f32,
}

/// The complete architecture graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureGraph {
    pub version: u32,
    pub project_path: String,
    pub project_name: String,
    pub generated_at: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub layers: HashMap<String, Vec<String>>,
    pub stats: GraphStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_files: u32,
    pub total_lines: u32,
    pub languages: HashMap<String, u32>,
    pub layers: HashMap<String, u32>,
    pub node_count: u32,
    pub edge_count: u32,
}

/// Change classification for incremental updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeClass {
    None,
    Cosmetic,
    Structural,
    Architecture,
    Full,
}

/// Fingerprint for a file (structural signature, not content hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub path: String,
    pub content_hash: String,
    pub structural_hash: String,
    pub line_count: u32,
    pub functions: Vec<String>,
    pub classes: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

// ══════════════════════════════════════════════════
// .omnixignore Support
// ══════════════════════════════════════════════════

/// Load .omnixignore patterns from a project directory
pub fn load_omnixignore(project_path: &PathBuf) -> Vec<String> {
    let ignore_path = project_path.join(".omnixignore");
    if !ignore_path.exists() {
        // Default ignore patterns
        return vec![
            "node_modules".into(),
            ".git".into(),
            "target".into(),
            "dist".into(),
            "build".into(),
            ".next".into(),
            "__pycache__".into(),
            ".venv".into(),
            "venv".into(),
            ".omnix".into(),
            ".understand-anything".into(),
        ];
    }

    fs::read_to_string(&ignore_path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Check if a path should be ignored
pub fn is_ignored(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        let p = pattern.trim_end_matches('/');
        if path.contains(p) || path.starts_with(p) {
            return true;
        }
        // Glob-like: *.ext
        if p.starts_with("*.") {
            let ext = &p[1..]; // ".ext"
            if path.ends_with(ext) {
                return true;
            }
        }
    }
    false
}

// ══════════════════════════════════════════════════
// Fingerprint & Change Detection
// ══════════════════════════════════════════════════

/// Compute structural fingerprint for a file
pub fn compute_fingerprint(path: &PathBuf) -> Option<FileFingerprint> {
    let content = fs::read_to_string(path).ok()?;
    let content_hash = compute_content_hash(&content);
    let line_count = content.lines().count() as u32;

    // Extract structural signatures
    let functions = extract_functions(&content);
    let classes = extract_classes(&content);
    let imports = extract_imports(&content);
    let exports = extract_exports(&content);

    // Structural hash = hash of signatures only (ignores comments, whitespace)
    let structural_content = format!(
        "fn:{}\nclass:{}\nimp:{}\nexp:{}",
        functions.join(","),
        classes.join(","),
        imports.join(","),
        exports.join(",")
    );
    let structural_hash = compute_content_hash(&structural_content);

    Some(FileFingerprint {
        path: path.to_string_lossy().to_string(),
        content_hash,
        structural_hash,
        line_count,
        functions,
        classes,
        imports,
        exports,
    })
}

/// Classify the change between old and new fingerprints
pub fn classify_change(old: Option<&FileFingerprint>, new: &FileFingerprint) -> ChangeClass {
    match old {
        None => ChangeClass::Structural, // New file
        Some(old_fp) => {
            if old_fp.structural_hash == new.structural_hash {
                if old_fp.content_hash == new.content_hash {
                    ChangeClass::None
                } else {
                    ChangeClass::Cosmetic
                }
            } else {
                // Structural change — check severity
                let func_diff = (old_fp.functions.len() as i32 - new.functions.len() as i32).abs();
                let class_diff = (old_fp.classes.len() as i32 - new.classes.len() as i32).abs();
                let import_diff = (old_fp.imports.len() as i32 - new.imports.len() as i32).abs();

                if func_diff > 5 || class_diff > 3 || import_diff > 5 {
                    ChangeClass::Architecture
                } else {
                    ChangeClass::Structural
                }
            }
        }
    }
}

// ══════════════════════════════════════════════════
// Architecture Layer Detection
// ══════════════════════════════════════════════════

/// Detect architecture layer from file path and content
pub fn detect_layer(path: &str, _content: &str) -> ArchLayer {
    let lower = path.to_lowercase();

    // Test files
    if lower.contains("test") || lower.contains("spec") || lower.contains("__tests__") {
        return ArchLayer::Test;
    }

    // Config files
    if lower.ends_with(".json") || lower.ends_with(".toml") || lower.ends_with(".yaml")
        || lower.ends_with(".yml") || lower.ends_with(".env") || lower.contains("config")
    {
        return ArchLayer::Config;
    }

    // API / Routes
    if lower.contains("api") || lower.contains("route") || lower.contains("controller")
        || lower.contains("handler") || lower.contains("endpoint")
    {
        return ArchLayer::Api;
    }

    // Service / Business logic
    if lower.contains("service") || lower.contains("business") || lower.contains("domain")
        || lower.contains("use_case") || lower.contains("usecase")
    {
        return ArchLayer::Service;
    }

    // Data layer
    if lower.contains("model") || lower.contains("entity") || lower.contains("schema")
        || lower.contains("migration") || lower.contains("repository") || lower.contains("database")
        || lower.contains("db")
    {
        return ArchLayer::Data;
    }

    // UI layer
    if lower.contains("component") || lower.contains("view") || lower.contains("page")
        || lower.contains("ui") || lower.contains("layout") || lower.contains("template")
        || lower.contains("widget")
    {
        return ArchLayer::Ui;
    }

    // Utility
    if lower.contains("util") || lower.contains("helper") || lower.contains("lib")
        || lower.contains("common") || lower.contains("shared")
    {
        return ArchLayer::Utility;
    }

    // Infrastructure
    if lower.contains("middleware") || lower.contains("proxy") || lower.contains("server")
        || lower.contains("deploy") || lower.contains("docker") || lower.contains("ci")
    {
        return ArchLayer::Infrastructure;
    }

    ArchLayer::Unknown
}

/// Detect language from file extension
pub fn detect_language(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("Rust".into()),
        "ts" | "tsx" => Some("TypeScript".into()),
        "js" | "jsx" | "mjs" => Some("JavaScript".into()),
        "py" => Some("Python".into()),
        "go" => Some("Go".into()),
        "java" => Some("Java".into()),
        "cpp" | "cc" | "cxx" => Some("C++".into()),
        "c" => Some("C".into()),
        "cs" => Some("C#".into()),
        "rb" => Some("Ruby".into()),
        "swift" => Some("Swift".into()),
        "kt" => Some("Kotlin".into()),
        "html" | "htm" => Some("HTML".into()),
        "css" | "scss" | "sass" => Some("CSS".into()),
        "json" => Some("JSON".into()),
        "md" | "mdx" => Some("Markdown".into()),
        "yaml" | "yml" => Some("YAML".into()),
        "toml" => Some("TOML".into()),
        "sql" => Some("SQL".into()),
        "sh" | "bash" | "zsh" => Some("Shell".into()),
        "lua" => Some("Lua".into()),
        _ => None,
    }
}

// ══════════════════════════════════════════════════
// Graph Builder
// ══════════════════════════════════════════════════

/// Build a complete architecture graph for a project directory
pub fn build_graph(project_path: &str) -> Result<ArchitectureGraph, String> {
    let root = PathBuf::from(project_path);
    if !root.exists() || !root.is_dir() {
        return Err(format!("Path does not exist or is not a directory: {}", project_path));
    }

    let ignore_patterns = load_omnixignore(&root);
    let project_name = root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut layers: HashMap<String, Vec<String>> = HashMap::new();
    let mut languages: HashMap<String, u32> = HashMap::new();
    let mut layer_counts: HashMap<String, u32> = HashMap::new();
    let mut total_lines = 0u32;
    let mut total_files = 0u32;

    // Walk directory tree
    walk_directory(
        &root,
        &root,
        &ignore_patterns,
        &mut nodes,
        &mut edges,
        &mut layers,
        &mut languages,
        &mut layer_counts,
        &mut total_lines,
        &mut total_files,
    );

    // Build import edges (simplified: match import statements to known node paths)
    build_import_edges(&mut edges, &nodes);

    Ok(ArchitectureGraph {
        version: 1,
        project_path: project_path.to_string(),
        project_name,
        generated_at: chrono::Utc::now().to_rfc3339(),
        nodes: nodes.clone(),
        edges: edges.clone(),
        layers,
        stats: GraphStats {
            total_files,
            total_lines,
            languages,
            layers: layer_counts,
            node_count: nodes.len() as u32,
            edge_count: edges.len() as u32,
        },
    })
}

fn walk_directory(
    root: &PathBuf,
    dir: &PathBuf,
    ignore: &[String],
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
    layers: &mut HashMap<String, Vec<String>>,
    languages: &mut HashMap<String, u32>,
    layer_counts: &mut HashMap<String, u32>,
    total_lines: &mut u32,
    total_files: &mut u32,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = relative.to_string_lossy().to_string();

        if is_ignored(&rel_str, ignore) {
            continue;
        }

        if path.is_dir() {
            // Add directory node
            let dir_name = path.file_name().unwrap().to_string_lossy().to_string();
            let dir_id = format!("dir:{}", rel_str);
            let layer = detect_layer(&rel_str, "");
            let layer_str = format!("{:?}", layer).to_lowercase();

            nodes.push(GraphNode {
                id: dir_id.clone(),
                name: dir_name,
                node_type: NodeType::Directory,
                path: rel_str.clone(),
                layer: layer.clone(),
                language: None,
                summary: None,
                line_count: 0,
                fingerprint: String::new(),
                complexity: None,
                tags: vec![],
            });

            layers.entry(layer_str.clone()).or_default().push(dir_id.clone());
            *layer_counts.entry(layer_str).or_insert(0) += 1;

            // Recurse
            walk_directory(root, &path, ignore, nodes, edges, layers, languages, layer_counts, total_lines, total_files);
        } else {
            // Add file node
            *total_files += 1;
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let file_id = format!("file:{}", rel_str);
            let layer = detect_layer(&rel_str, "");
            let layer_str = format!("{:?}", layer).to_lowercase();
            let lang = detect_language(&rel_str);

            let (line_count, fingerprint) = if let Some(fp) = compute_fingerprint(&path) {
                (fp.line_count, fp.structural_hash)
            } else {
                (0, String::new())
            };

            *total_lines += line_count;

            if let Some(ref l) = lang {
                *languages.entry(l.clone()).or_insert(0) += 1;
            }

            nodes.push(GraphNode {
                id: file_id.clone(),
                name: file_name,
                node_type: node_type_from_path(&rel_str),
                path: rel_str.clone(),
                layer: layer.clone(),
                language: lang,
                summary: None,
                line_count,
                fingerprint,
                complexity: None,
                tags: vec![],
            });

            layers.entry(layer_str.clone()).or_default().push(file_id.clone());
            *layer_counts.entry(layer_str).or_insert(0) += 1;

            // Add contains edge from parent directory
            if let Some(parent) = path.parent() {
                let parent_rel = parent.strip_prefix(root).unwrap_or(parent);
                let parent_id = format!("dir:{}", parent_rel.to_string_lossy());
                edges.push(GraphEdge {
                    source: parent_id,
                    target: file_id,
                    edge_type: EdgeType::Contains,
                    weight: 1.0,
                });
            }
        }
    }
}

/// Build import edges by matching import statements to known node paths
fn build_import_edges(edges: &mut Vec<GraphEdge>, nodes: &[GraphNode]) {
    let path_map: HashMap<String, &str> = nodes.iter()
        .map(|n| (n.path.clone(), n.id.as_str()))
        .collect();

    for node in nodes {
        if node.node_type == NodeType::File {
            let path = PathBuf::from(&node.path);
            if let Ok(content) = fs::read_to_string(&path) {
                let imports = extract_imports(&content);
                for import_path in imports {
                    // Try to resolve import to a known file
                    if let Some(target_id) = resolve_import(&import_path, &node.path, &path_map) {
                        edges.push(GraphEdge {
                            source: node.id.clone(),
                            target: target_id.to_string(),
                            edge_type: EdgeType::Imports,
                            weight: 1.0,
                        });
                    }
                }
            }
        }
    }
}

/// Resolve an import path to a node ID
fn resolve_import<'a>(import: &str, from_path: &str, path_map: &'a HashMap<String, &'a str>) -> Option<&'a str> {
    // Skip external packages
    if !import.starts_with('.') && !import.starts_with('/') {
        return None;
    }

    // Try common extensions
    let base = if import.starts_with('.') {
        let from_dir = std::path::Path::new(from_path).parent()?;
        from_dir.join(import).to_string_lossy().to_string()
    } else {
        import.to_string()
    };

    for ext in &["", ".ts", ".tsx", ".js", ".jsx", ".rs", ".py", "/index.ts", "/index.tsx", "/index.js", "/mod.rs"] {
        let candidate = format!("{}{}", base, ext);
        if path_map.contains_key(&candidate) {
            return path_map.get(&candidate).copied();
        }
    }

    None
}

/// Determine NodeType from file path
fn node_type_from_path(path: &str) -> NodeType {
    let lower = path.to_lowercase();
    if lower.contains("test") || lower.contains("spec") { return NodeType::Test; }
    if lower.contains("config") || lower.ends_with(".json") || lower.ends_with(".toml") || lower.ends_with(".yaml") { return NodeType::Config; }
    if lower.contains("route") || lower.contains("api") { return NodeType::Route; }
    if lower.contains("component") || lower.contains("view") { return NodeType::Component; }
    if lower.contains("hook") || lower.contains("use_") { return NodeType::Hook; }
    if lower.contains("style") || lower.ends_with(".css") || lower.ends_with(".scss") { return NodeType::Style; }
    if lower.contains("util") || lower.contains("helper") || lower.contains("lib") { return NodeType::Module; }
    NodeType::File
}

/// Simple function name extraction (works for JS/TS/Rust/Python)
fn extract_functions(content: &str) -> Vec<String> {
    let mut funcs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // JS/TS: function name( or const name = ( or export function name(
        if let Some(name) = extract_js_func(trimmed) {
            funcs.push(name);
        }
        // Rust: fn name( or pub fn name(
        if let Some(name) = extract_rust_func(trimmed) {
            funcs.push(name);
        }
        // Python: def name(
        if let Some(name) = extract_python_func(trimmed) {
            funcs.push(name);
        }
    }
    funcs.sort();
    funcs.dedup();
    funcs
}

fn extract_js_func(line: &str) -> Option<String> {
    let patterns = ["export async function ", "export function ", "async function ", "function "];
    for pat in &patterns {
        if line.starts_with(pat) || line.contains(pat) {
            let rest = if let Some(pos) = line.find(pat) { &line[pos + pat.len()..] } else { continue };
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
            if !name.is_empty() && name.len() > 1 { return Some(name); }
        }
    }
    // Arrow functions: const name = (...) =>
    if let Some(eq_pos) = line.find(" = ") {
        let before = &line[..eq_pos].trim();
        if let Some(name) = before.split_whitespace().last() {
            let name = name.trim();
            if name.len() > 1 && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                if line.contains("=>") || line.contains("function(") {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn extract_rust_func(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("pub async fn ") || trimmed.starts_with("pub fn ") || trimmed.starts_with("async fn ") || trimmed.starts_with("fn ") {
        let rest = if trimmed.starts_with("pub async fn ") { &trimmed[13..] }
            else if trimmed.starts_with("pub fn ") { &trimmed[7..] }
            else if trimmed.starts_with("async fn ") { &trimmed[9..] }
            else if trimmed.starts_with("fn ") { &trimmed[3..] }
            else { return None };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !name.is_empty() { return Some(name); }
    }
    None
}

fn extract_python_func(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
        let rest = if trimmed.starts_with("async def ") { &trimmed[10..] } else { &trimmed[4..] };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !name.is_empty() { return Some(name); }
    }
    None
}

fn extract_classes(content: &str) -> Vec<String> {
    let mut classes = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ") || trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
            let rest = if trimmed.starts_with("pub struct ") { &trimmed[11..] }
                else if trimmed.starts_with("struct ") { &trimmed[7..] }
                else { &trimmed[6..] };
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '<').collect();
            if !name.is_empty() { classes.push(name); }
        }
    }
    classes.sort();
    classes.dedup();
    classes
}

fn extract_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // JS/TS: import ... from '...'
        if trimmed.starts_with("import ") && trimmed.contains(" from ") {
            if let Some(from_pos) = trimmed.find(" from ") {
                let path_part = &trimmed[from_pos + 6..].trim().trim_matches('\'').trim_matches('"').trim_end_matches(';');
                if path_part.starts_with('.') || path_part.starts_with('/') {
                    imports.push(path_part.to_string());
                }
            }
        }
        // Rust: use crate::...
        if trimmed.starts_with("use crate::") {
            let path = trimmed.trim_start_matches("use ").trim_end_matches(';');
            imports.push(path.to_string());
        }
        // Python: from ... import ...
        if trimmed.starts_with("from ") && trimmed.contains(" import ") {
            if let Some(import_pos) = trimmed.find(" import ") {
                let module = &trimmed[5..import_pos].trim();
                if module.starts_with('.') {
                    imports.push(module.to_string());
                }
            }
        }
    }
    imports
}

fn extract_exports(content: &str) -> Vec<String> {
    let mut exports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("export ") || trimmed.starts_with("pub fn ") || trimmed.starts_with("pub struct ") || trimmed.starts_with("pub enum ") {
            exports.push(trimmed.to_string());
        }
    }
    exports
}

/// Simple FNV-1a content hash
fn compute_content_hash(content: &str) -> String {
    let data = content.as_bytes();
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv-{:016x}", hash)
}

/// Save architecture graph to disk
pub fn save_graph(graph: &ArchitectureGraph) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let graphs_dir = home.join(".omnix").join("architecture");
    fs::create_dir_all(&graphs_dir).map_err(|e| e.to_string())?;

    // Use project name as filename
    let filename = format!("{}.json", graph.project_name.replace(['/', '\\', ':'], "_"));
    let path = graphs_dir.join(&filename);
    let json = serde_json::to_string_pretty(graph).map_err(|e| e.to_string())?;
    fs::write(&path, &json).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

/// Load a saved architecture graph
pub fn load_graph(project_name: &str) -> Result<ArchitectureGraph, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let filename = format!("{}.json", project_name.replace(['/', '\\', ':'], "_"));
    let path = home.join(".omnix").join("architecture").join(&filename);

    if !path.exists() {
        return Err(format!("No saved graph for project: {}", project_name));
    }

    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse graph: {}", e))
}
