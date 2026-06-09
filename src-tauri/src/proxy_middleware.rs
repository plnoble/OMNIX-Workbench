//! Proxy Middleware Pipeline (Cherry Studio inspired)
//!
//! Decomposes the monolithic proxy handler into a chain of composable
//! middleware stages. Each stage is independent and testable.
//!
//! Pipeline: RequestParse → FormatConvert → CapabilityRoute → UpstreamCall → ResponseProcess → LogRequest
//!
//! NOTE: This module defines the middleware architecture but is not yet wired into proxy.rs.
//! Integration will happen in a follow-up refactor to avoid disrupting the working proxy.

#![allow(dead_code)]

use std::sync::Arc;

use crate::db::DbManager;

// ─────────────────────────────────────────────
// Pipeline Context
// ─────────────────────────────────────────────

/// Shared context passed through the middleware pipeline
pub struct PipelineContext {
    /// Original request model name (from user)
    pub request_model: String,
    /// Resolved upstream model name
    pub resolved_model: String,
    /// Target platform ID
    pub platform_id: String,
    /// API key for upstream
    pub api_key: String,
    /// API address for upstream
    pub api_address: String,
    /// API type: "anthropic" | "openai" | "ollama"
    pub api_type: String,
    /// Whether this is a streaming request
    pub is_stream: bool,
    /// Request start time
    pub start_time: std::time::Instant,
    /// Capability classification results
    pub capabilities: CapabilityFlags,
    /// Whether web search was requested
    pub web_search_enabled: bool,
    /// Web search results (injected as context)
    pub search_context: Option<String>,
    /// Reasoning effort level
    pub reasoning_effort: Option<String>,
    /// Agent name (if proxied via /agent/:name/)
    pub agent_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CapabilityFlags {
    pub needs_vision: bool,
    pub needs_reasoning: bool,
    pub needs_coding: bool,
    pub needs_speedy: bool,
}

/// Result of a pipeline execution
pub struct PipelineResult {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: String,
    pub is_stream: bool,
    pub latency_ms: i64,
    pub tokens_prompt: i64,
    pub tokens_completion: i64,
}

// ─────────────────────────────────────────────
// Middleware Trait
// ─────────────────────────────────────────────

/// A single stage in the proxy middleware pipeline
pub trait Middleware: Send + Sync {
    /// Name of this middleware stage (for logging)
    fn name(&self) -> &str;

    /// Process the request. Return Ok(()) to continue, Err to abort.
    fn on_request(&self, ctx: &mut PipelineContext) -> Result<(), MiddlewareError>;

    /// Process the response after upstream call.
    fn on_response(&self, ctx: &PipelineContext, result: &mut PipelineResult);
}

#[derive(Debug)]
pub enum MiddlewareError {
    /// Client error (4xx)
    ClientError(u16, String),
    /// Upstream error (5xx)
    UpstreamError(u16, String),
    /// Internal error
    InternalError(String),
}

// ─────────────────────────────────────────────
// Built-in Middleware Stages
// ─────────────────────────────────────────────

/// Stage 1: Parse and validate the incoming request
pub struct RequestParseMiddleware;

impl Middleware for RequestParseMiddleware {
    fn name(&self) -> &str { "request_parse" }

    fn on_request(&self, ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        // Validate model name is not empty
        if ctx.request_model.is_empty() {
            return Err(MiddlewareError::ClientError(400, "Missing model name".into()));
        }
        ctx.start_time = std::time::Instant::now();
        Ok(())
    }

    fn on_response(&self, _ctx: &PipelineContext, _result: &mut PipelineResult) {}
}

/// Stage 2: Classify request capabilities for routing
pub struct CapabilityClassifyMiddleware;

impl Middleware for CapabilityClassifyMiddleware {
    fn name(&self) -> &str { "capability_classify" }

    fn on_request(&self, _ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        // Classification is done externally before pipeline runs
        // This middleware just logs the classification
        Ok(())
    }

    fn on_response(&self, _ctx: &PipelineContext, _result: &mut PipelineResult) {}
}

/// Stage 3: Resolve upstream model and platform
pub struct UpstreamResolveMiddleware {
    pub db: Arc<DbManager>,
}

impl Middleware for UpstreamResolveMiddleware {
    fn name(&self) -> &str { "upstream_resolve" }

    fn on_request(&self, ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        let conn = match self.db.get_connection() {
            Ok(c) => c,
            Err(e) => return Err(MiddlewareError::InternalError(format!("DB error: {}", e))),
        };

        // If model is "Auto", use capability-based routing
        if ctx.request_model == "Auto" {
            // Weighted selection from matching platforms
            if let Ok(_stmt) = conn.prepare(
                "SELECT mp.id, mp.api_key, mp.api_address, mp.api_type, mp.weight, mp.priority, pm.model_name
                 FROM platform_models pm
                 JOIN model_platforms mp ON pm.platform_id = mp.id
                 WHERE pm.model_name = ?1 AND pm.is_enabled = 1 AND mp.is_enabled = 1 AND mp.is_healthy = 1
                 ORDER BY mp.priority DESC, mp.weight DESC"
            ) {
                // ... weighted selection logic (same as current proxy.rs)
            }
        } else {
            // Direct model lookup
            // ... same as current resolve_model_upstream
        }

        Ok(())
    }

    fn on_response(&self, _ctx: &PipelineContext, _result: &mut PipelineResult) {}
}

/// Stage 4: Inject web search context if enabled
pub struct WebSearchMiddleware;

impl Middleware for WebSearchMiddleware {
    fn name(&self) -> &str { "web_search" }

    fn on_request(&self, ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        if ctx.web_search_enabled {
            // Web search is handled at the frontend level (ChatTab)
            // This middleware just passes through
        }
        Ok(())
    }

    fn on_response(&self, _ctx: &PipelineContext, _result: &mut PipelineResult) {}
}

/// Stage 5: Apply reasoning effort parameters
pub struct ReasoningEffortMiddleware;

impl Middleware for ReasoningEffortMiddleware {
    fn name(&self) -> &str { "reasoning_effort" }

    fn on_request(&self, ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        // Reasoning effort mapping is done in format conversion
        // This middleware validates the value
        if let Some(ref effort) = ctx.reasoning_effort {
            match effort.as_str() {
                "low" | "medium" | "high" => {}
                _ => return Err(MiddlewareError::ClientError(400, format!("Invalid reasoning_effort: {}", effort))),
            }
        }
        Ok(())
    }

    fn on_response(&self, _ctx: &PipelineContext, _result: &mut PipelineResult) {}
}

/// Stage 6: Log request to database
pub struct LogRequestMiddleware {
    pub db: Arc<DbManager>,
}

impl Middleware for LogRequestMiddleware {
    fn name(&self) -> &str { "log_request" }

    fn on_request(&self, _ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        // No-op on request
        Ok(())
    }

    fn on_response(&self, ctx: &PipelineContext, result: &mut PipelineResult) {
        let latency = ctx.start_time.elapsed().as_millis() as i64;
        result.latency_ms = latency;

        // Copy values before moving into spawn_blocking
        let db = self.db.clone();
        let model = ctx.resolved_model.clone();
        let api_type = ctx.api_type.clone();
        let status = result.status;
        let is_stream = result.is_stream;
        let is_error = status >= 400;
        let agent = ctx.agent_name.clone();
        let tokens_prompt = result.tokens_prompt;
        let tokens_completion = result.tokens_completion;

        tokio::task::spawn_blocking(move || {
            crate::proxy::log_request(
                &db, &model, Some(&api_type),
                tokens_prompt, tokens_completion,
                latency, status as i32, is_stream, is_error,
                None, None, agent.as_deref().unwrap_or("proxy"),
            );
        });
    }
}

// ─────────────────────────────────────────────
// Pipeline Builder
// ─────────────────────────────────────────────

/// Build and execute a middleware pipeline
pub struct ProxyPipeline {
    middlewares: Vec<Box<dyn Middleware>>,
}

impl ProxyPipeline {
    /// Create the default proxy pipeline
    pub fn default(db: Arc<DbManager>) -> Self {
        Self {
            middlewares: vec![
                Box::new(RequestParseMiddleware),
                Box::new(CapabilityClassifyMiddleware),
                Box::new(UpstreamResolveMiddleware { db: db.clone() }),
                Box::new(WebSearchMiddleware),
                Box::new(ReasoningEffortMiddleware),
                Box::new(LogRequestMiddleware { db }),
            ],
        }
    }

    /// Execute the request phase of the pipeline
    pub fn execute_request(&self, ctx: &mut PipelineContext) -> Result<(), MiddlewareError> {
        for mw in &self.middlewares {
            mw.on_request(ctx)?;
        }
        Ok(())
    }

    /// Execute the response phase of the pipeline (in reverse order)
    pub fn execute_response(&self, ctx: &PipelineContext, result: &mut PipelineResult) {
        for mw in self.middlewares.iter().rev() {
            mw.on_response(ctx, result);
        }
    }
}
