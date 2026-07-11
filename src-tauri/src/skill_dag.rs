//! Skill DAG — Typed Dependency Graph for Skills
//!
//! Models inter-skill relationships as a typed directed acyclic graph:
//! - depends_on: A requires B as precondition (directed, DAG)
//! - specializes: A is a narrower version of B (directed, DAG)
//! - composes_with: A and B chain in workflows (symmetric)
//! - similar_to: A and B are functionally redundant (symmetric)
//! - conflicts_with: A + B causes predictable failure (symmetric, non-walkable)
//!
//! Provides conflict-aware retrieval, set validation, propose/commit mutations,
//! cycle detection, and rollback with audit trail.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

// ══════════════════════════════════════════════════
// Edge Types
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeType {
    /// A requires B as precondition (directed, DAG)
    DependsOn,
    /// A is a narrower version of B (directed, DAG)
    Specializes,
    /// A and B chain in workflows (symmetric)
    ComposesWith,
    /// A and B are functionally redundant (symmetric)
    SimilarTo,
    /// A + B causes predictable failure (symmetric, non-walkable)
    ConflictsWith,
}

impl EdgeType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "depends_on" => Some(Self::DependsOn),
            "specializes" => Some(Self::Specializes),
            "composes_with" => Some(Self::ComposesWith),
            "similar_to" => Some(Self::SimilarTo),
            "conflicts_with" => Some(Self::ConflictsWith),
            _ => None,
        }
    }

    /// Whether this edge type is traversable in search
    pub fn is_walkable(&self) -> bool {
        self != &Self::ConflictsWith
    }

    /// Whether this edge type is directed (forms DAG backbone)
    pub fn is_directed(&self) -> bool {
        matches!(self, Self::DependsOn | Self::Specializes)
    }

    /// Whether this edge type is symmetric
    pub fn is_symmetric(&self) -> bool {
        !self.is_directed()
    }
}

// ══════════════════════════════════════════════════
// Edge & Graph
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEdge {
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub reason: String,
    pub origin: String,  // "manual" | "auto" | "imported"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillNode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub status: String,  // "active" | "inactive"
    pub tags: Vec<String>,
}

/// The skill dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillGraph {
    pub nodes: HashMap<String, SkillNode>,
    pub edges: Vec<SkillEdge>,
    pub history: Vec<GraphMutation>,
}

/// Mutation audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMutation {
    pub action: String,  // "add" | "remove" | "retype"
    pub edge: SkillEdge,
    pub reason: String,
    pub timestamp: i64,
    pub task_id: Option<String>,
}

/// Search result with conflict awareness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchResult {
    pub matches: Vec<String>,
    pub neighbors: Vec<String>,
    pub conflicts: Vec<ConflictPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictPair {
    pub skill_a: String,
    pub skill_b: String,
    pub reason: String,
}

/// Set validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetValidation {
    pub valid: bool,
    pub missing_deps: Vec<String>,
    pub conflicts: Vec<ConflictPair>,
    pub redundant: Vec<(String, String)>,
    pub suggestions: Vec<String>,
}

impl SkillGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            history: Vec::new(),
        }
    }

    /// Build graph from OMNIX skills table
    pub fn from_skills(skills: &[(String, String, String, String)]) -> Self {
        let mut graph = Self::new();
        for (name, description, path, dependencies) in skills {
            graph.nodes.insert(name.clone(), SkillNode {
                id: name.clone(),
                name: name.clone(),
                description: description.clone(),
                path: path.clone(),
                status: "active".into(),
                tags: vec![],
            });

            // Parse existing dependencies as depends_on edges
            if let Ok(deps) = serde_json::from_str::<Vec<String>>(dependencies) {
                for dep in deps {
                    graph.edges.push(SkillEdge {
                        source: name.clone(),
                        target: dep,
                        edge_type: EdgeType::DependsOn,
                        reason: "imported from dependencies field".into(),
                        origin: "imported".into(),
                    });
                }
            }
        }
        graph
    }

    // ── Cycle Detection ────────────────────────────────

    /// Check if adding an edge would create a cycle in the DAG backbone
    pub fn would_create_cycle(&self, source: &str, target: &str, edge_type: &EdgeType) -> bool {
        if !edge_type.is_directed() {
            return false; // Symmetric edges don't create cycles
        }
        // BFS: can we reach source from target via existing directed edges?
        self.is_reachable(target, source)
    }

    /// Check if target is reachable from source via directed edges
    fn is_reachable(&self, from: &str, to: &str) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == to {
                return true;
            }
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            for edge in &self.edges {
                if edge.edge_type.is_directed() && edge.source == current {
                    queue.push_back(edge.target.clone());
                }
            }
        }
        false
    }

    // ── Propose / Commit ───────────────────────────────

    /// Commit an edge addition
    pub fn commit_add(&mut self, edge: SkillEdge) -> bool {
        // Check for duplicates
        let exists = self.edges.iter().any(|e|
            e.source == edge.source && e.target == edge.target && e.edge_type == edge.edge_type
        );
        if exists { return false; }

        // For symmetric edges, add canonical form
        if edge.edge_type.is_symmetric() {
            let (s, t) = if edge.source <= edge.target {
                (edge.source.clone(), edge.target.clone())
            } else {
                (edge.target.clone(), edge.source.clone())
            };
            let canonical = SkillEdge {
                source: s,
                target: t,
                ..edge.clone()
            };
            self.edges.push(canonical);
        } else {
            self.edges.push(edge.clone());
        }

        self.history.push(GraphMutation {
            action: "add".into(),
            edge,
            reason: String::new(),
            timestamp: chrono::Utc::now().timestamp(),
            task_id: None,
        });
        true
    }

    /// Remove an edge
    pub fn commit_remove(&mut self, source: &str, target: &str, edge_type: &EdgeType) -> bool {
        let before = self.edges.len();
        self.edges.retain(|e| {
            if e.edge_type.is_symmetric() {
                !(e.edge_type == *edge_type &&
                  ((e.source == source && e.target == target) ||
                   (e.source == target && e.target == source)))
            } else {
                !(e.source == source && e.target == target && e.edge_type == *edge_type)
            }
        });
        self.edges.len() < before
    }

    // ── Conflict-Aware Search ──────────────────────────

    /// Search with conflict awareness
    pub fn search(&self, query: &str, top_k: usize) -> SkillSearchResult {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        // Score each node
        let mut scored: Vec<(&SkillNode, f32)> = self.nodes.values().map(|node| {
            let name_lower = node.name.to_lowercase();
            let desc_lower = node.description.to_lowercase();
            let mut score = 0.0f32;

            if name_lower.contains(&query_lower) { score += 10.0; }
            for word in &query_words {
                if name_lower.contains(word) { score += 3.0; }
                if desc_lower.contains(word) { score += 1.0; }
            }
            (node, score)
        }).collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let matches: Vec<String> = scored.iter()
            .filter(|(_, s)| *s >= 2.0)
            .take(top_k)
            .map(|(n, _)| n.id.clone())
            .collect();

        // BFS neighbors (walkable edges only)
        let mut neighbors = HashSet::new();
        for m in &matches {
            for edge in &self.edges {
                if !edge.edge_type.is_walkable() { continue; }
                if edge.source == *m && !matches.contains(&edge.target) {
                    neighbors.insert(edge.target.clone());
                }
                if edge.target == *m && !matches.contains(&edge.source) {
                    neighbors.insert(edge.source.clone());
                }
            }
        }

        // Find conflicts
        let mut conflicts = Vec::new();
        for m in &matches {
            for edge in &self.edges {
                if edge.edge_type == EdgeType::ConflictsWith {
                    if edge.source == *m {
                        conflicts.push(ConflictPair {
                            skill_a: m.clone(),
                            skill_b: edge.target.clone(),
                            reason: edge.reason.clone(),
                        });
                    } else if edge.target == *m {
                        conflicts.push(ConflictPair {
                            skill_a: m.clone(),
                            skill_b: edge.source.clone(),
                            reason: edge.reason.clone(),
                        });
                    }
                }
            }
        }

        SkillSearchResult {
            matches,
            neighbors: neighbors.into_iter().collect(),
            conflicts,
        }
    }

    // ── Set Validation ─────────────────────────────────

    /// Validate a skill set: check deps, conflicts, redundancy
    pub fn check_set(&self, skill_ids: &[String]) -> SetValidation {
        let set: HashSet<&String> = skill_ids.iter().collect();
        let mut missing_deps = Vec::new();
        let mut conflicts = Vec::new();
        let mut redundant = Vec::new();

        // Check depends_on
        for id in skill_ids {
            for edge in &self.edges {
                if edge.edge_type == EdgeType::DependsOn && edge.source == *id && !set.contains(&edge.target) {
                    missing_deps.push(edge.target.clone());
                }
            }
        }

        // Check conflicts
        for edge in &self.edges {
            if edge.edge_type == EdgeType::ConflictsWith {
                if set.contains(&edge.source) && set.contains(&edge.target) {
                    conflicts.push(ConflictPair {
                        skill_a: edge.source.clone(),
                        skill_b: edge.target.clone(),
                        reason: edge.reason.clone(),
                    });
                }
            }
        }

        // Check redundancy (similar_to / specializes within set)
        for edge in &self.edges {
            if edge.edge_type == EdgeType::SimilarTo || edge.edge_type == EdgeType::Specializes {
                if set.contains(&edge.source) && set.contains(&edge.target) {
                    redundant.push((edge.source.clone(), edge.target.clone()));
                }
            }
        }

        let valid = missing_deps.is_empty() && conflicts.is_empty();

        SetValidation {
            valid,
            missing_deps,
            conflicts,
            redundant,
            suggestions: if !valid { vec!["Fix missing dependencies and conflicts before proceeding".into()] } else { vec![] },
        }
    }

    /// Expand a skill set with all transitive depends_on prerequisites
    pub fn expand_set(&self, skill_ids: &[String]) -> Vec<String> {
        let mut expanded: HashSet<String> = skill_ids.iter().cloned().collect();
        let mut queue: VecDeque<String> = skill_ids.iter().cloned().collect();

        while let Some(current) = queue.pop_front() {
            for edge in &self.edges {
                if edge.edge_type == EdgeType::DependsOn && edge.source == current && !expanded.contains(&edge.target) {
                    expanded.insert(edge.target.clone());
                    queue.push_back(edge.target.clone());
                }
            }
        }

        expanded.into_iter().collect()
    }
}
