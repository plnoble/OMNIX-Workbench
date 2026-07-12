//! Gateway wire types: Anthropic & OpenAI request/response DTOs shared by the
//! proxy handlers. Split out of proxy.rs; `proxy.rs` re-exports everything so
//! existing `crate::proxy::*` paths keep working.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicMessageContent {
    String(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    /// Anthropic image source (`{type:"base64", media_type, data}`) — kept as
    /// raw JSON so vision content survives the gateway translation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<serde_json::Value>,
}

impl AnthropicMessageContent {
    pub fn to_string_content(&self) -> String {
        match self {
            AnthropicMessageContent::String(s) => s.clone(),
            AnthropicMessageContent::Blocks(blocks) => {
                let mut text_parts = Vec::new();
                for block in blocks {
                    if block.block_type == "text" {
                        if let Some(ref t) = block.text {
                            text_parts.push(t.as_str());
                        }
                    }
                }
                text_parts.join("\n")
            }
        }
    }

    /// OpenAI chat-completions content: a plain string for text-only messages
    /// (identical to the old behavior), or a parts array when image blocks are
    /// present — base64 sources become `image_url` data URLs so vision inputs
    /// are no longer dropped by the gateway translation.
    pub fn to_openai_content(&self) -> serde_json::Value {
        let AnthropicMessageContent::Blocks(blocks) = self else {
            return serde_json::Value::String(self.to_string_content());
        };
        let has_images = blocks.iter().any(|block| block.block_type == "image");
        if !has_images {
            return serde_json::Value::String(self.to_string_content());
        }
        let mut parts = Vec::new();
        for block in blocks {
            match block.block_type.as_str() {
                "text" => {
                    if let Some(text) = &block.text {
                        parts.push(serde_json::json!({ "type": "text", "text": text }));
                    }
                }
                "image" => {
                    if let Some(source) = &block.source {
                        let media_type = source
                            .get("media_type")
                            .and_then(|value| value.as_str())
                            .unwrap_or("image/png");
                        if let Some(data) = source.get("data").and_then(|value| value.as_str()) {
                            parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": format!("data:{media_type};base64,{data}") },
                            }));
                        } else if let Some(url) =
                            source.get("url").and_then(|value| value.as_str())
                        {
                            parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": url },
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
        serde_json::Value::Array(parts)
    }
}

// Anthropic Request format
#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicMessageContent,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: Option<u32>,
    pub system: Option<AnthropicMessageContent>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
    /// Reasoning effort control: "low" | "medium" | "high"
    /// Maps to budget_tokens for Anthropic extended thinking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

// OpenAI Request format
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OpenAIRequestMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct OpenAIRequest {
    pub(crate) model: String,
    /// Plain-string content for text messages; a parts array when images ride
    /// along (see `AnthropicMessageContent::to_openai_content`).
    pub(crate) messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stream: Option<bool>,
}

// Structs for parsing OpenAI responses
#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIChoiceDelta {
    pub(crate) content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIChoice {
    pub(crate) delta: OpenAIChoiceDelta,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamChunk {
    pub(crate) choices: Vec<OpenAIChoice>,
}
