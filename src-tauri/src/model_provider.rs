//! Model Provider Abstraction Layer (Cherry Studio + New API inspired)
//!
//! Defines a unified `ModelProvider` trait that each platform implements.
//! OpenAI-compatible format serves as the canonical wire format.
//! New platforms only need to implement this trait — no proxy.rs changes needed.
//!
//! INTEGRATION STATUS: Types are used by proxy_middleware.rs for pipeline context.
//! Concrete provider implementations (OpenAI, Anthropic, etc.) will be added
//! as the proxy migration progresses.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────
// Canonical Request/Response Types
// ─────────────────────────────────────────────

/// Unified chat completion request (OpenAI-compatible wire format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
    /// Reasoning effort: "low" | "medium" | "high"
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub source: Option<ImageSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// Unified chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    /// For reasoning models: thinking content
    #[serde(rename = "thinking")]
    pub thinking: Option<String>,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub index: u32,
    pub embedding: Vec<f32>,
}

/// Model info from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub owned_by: String,
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    pub chat: bool,
    pub embedding: bool,
    pub vision: bool,
    pub reasoning: bool,
    pub function_calling: bool,
}

// ─────────────────────────────────────────────
// Provider Trait
// ─────────────────────────────────────────────

/// Unified model provider interface
/// Each platform (OpenAI, Anthropic, Gemini, Ollama, etc.) implements this trait
pub trait ModelProvider: Send + Sync {
    /// Unique provider identifier
    fn id(&self) -> &str;

    /// Human-readable display name
    fn display_name(&self) -> &str;

    /// Provider type: "openai" | "anthropic" | "google" | "ollama" | "custom"
    fn provider_type(&self) -> &str;

    /// Whether this provider is currently healthy
    fn is_healthy(&self) -> bool;

    /// Base URL for API calls
    fn base_url(&self) -> &str;

    /// Send a chat completion request (non-streaming)
    fn chat_completion(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError>;

    /// Send a streaming chat completion request
    fn chat_completion_stream(&self, request: &ChatRequest) -> Result<Vec<StreamChunk>, ProviderError>;

    /// Generate embeddings
    fn embeddings(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>;

    /// List available models
    fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;

    /// Check if this provider supports a specific capability
    fn supports(&self, capability: &str) -> bool {
        match capability {
            "chat" => true,
            "stream" => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum ProviderError {
    /// Authentication failed
    AuthError(String),
    /// Rate limited
    RateLimitError(u32), // retry_after seconds
    /// Model not found
    ModelNotFound(String),
    /// Network error
    NetworkError(String),
    /// Invalid request
    BadRequest(String),
    /// Server error
    ServerError(u16, String),
    /// Timeout
    Timeout,
    /// Other
    Other(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::AuthError(msg) => write!(f, "Auth error: {}", msg),
            ProviderError::RateLimitError(secs) => write!(f, "Rate limited, retry after {}s", secs),
            ProviderError::ModelNotFound(model) => write!(f, "Model not found: {}", model),
            ProviderError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ProviderError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            ProviderError::ServerError(code, msg) => write!(f, "Server error {}: {}", code, msg),
            ProviderError::Timeout => write!(f, "Request timed out"),
            ProviderError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

// ─────────────────────────────────────────────
// Provider Registry
// ─────────────────────────────────────────────

/// Registry of all configured model providers
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ModelProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self { providers: HashMap::new() }
    }

    /// Register a provider
    pub fn register(&mut self, provider: Box<dyn ModelProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    /// Get a provider by ID
    pub fn get(&self, id: &str) -> Option<&Box<dyn ModelProvider>> {
        self.providers.get(id)
    }

    /// Get all providers
    pub fn all(&self) -> Vec<&Box<dyn ModelProvider>> {
        self.providers.values().collect()
    }

    /// Get all healthy providers
    pub fn healthy(&self) -> Vec<&Box<dyn ModelProvider>> {
        self.providers.values().filter(|p| p.is_healthy()).collect()
    }

    /// Find provider that serves a specific model
    pub fn find_for_model(&self, model_id: &str) -> Option<&Box<dyn ModelProvider>> {
        for provider in self.providers.values() {
            if let Ok(models) = provider.list_models() {
                if models.iter().any(|m| m.id == model_id) {
                    return Some(provider);
                }
            }
        }
        None
    }

    /// Auto-route: find the best provider for a request based on capabilities
    pub fn auto_route(&self, request: &ChatRequest) -> Option<&Box<dyn ModelProvider>> {
        let needs_reasoning = request.reasoning_effort.is_some();
        let needs_vision = request.messages.iter().any(|m| {
            match &m.content {
                MessageContent::Blocks(blocks) => blocks.iter().any(|b| b.block_type == "image"),
                _ => false,
            }
        });

        // Score each healthy provider
        let mut best: Option<(&Box<dyn ModelProvider>, i32)> = None;
        for provider in self.healthy() {
            let mut score = 0;
            if needs_reasoning && provider.supports("reasoning") { score += 10; }
            if needs_vision && provider.supports("vision") { score += 10; }
            if provider.supports("chat") { score += 1; }

            if score > 0 {
                if best.is_none() || score > best.as_ref().unwrap().1 {
                    best = Some((provider, score));
                }
            }
        }

        best.map(|(p, _)| p)
    }
}
