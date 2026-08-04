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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    /// `null` 不能出现在转发出去的请求里：给 tool_use 块加一个 `"text":null`
    /// 会让严格的上游把它当成畸形内容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Anthropic image source (`{type:"base64", media_type, data}`) — kept as
    /// raw JSON so vision content survives the gateway translation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<serde_json::Value>,
    /// **认不出来的字段一律原样带走。**
    ///
    /// 这个网关会把请求反序列化成本结构体、再重新序列化转发（proxy.rs 的
    /// `.json(&payload)`），所以结构体里没写的字段会**静默消失**。曾经因此
    /// 把 `tool_use` 的 id/name/input 和 `tool_result` 的 tool_use_id 全丢掉——
    /// 转发出去的是 `{"type":"tool_use","text":null}`，工具调用链直接断掉。
    ///
    /// 逐个补字段是打地鼠：下一个新块类型（thinking、citations、
    /// cache_control…）还会再丢一次。所以这里用兜底，修的是这一**类**问题。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
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

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    // 以下几个都要 skip：转发一个 `"temperature":null` 给上游，跟没写它是两回事。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<AnthropicMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Reasoning effort control: "low" | "medium" | "high"
    /// Maps to budget_tokens for Anthropic extended thinking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// **认不出来的字段一律原样带走**——尤其是 `tools` 和 `tool_choice`。
    ///
    /// 这个结构体原本没有 `tools`，而 OMNIX 把 agent CLI 指到自己网关上
    /// （`ANTHROPIC_BASE_URL=.../agent/<名字>`），于是 Claude Code 声明的工具
    /// 在转发那一步被整个吃掉，上游模型根本不知道有工具可用。
    ///
    /// 见 `wire_fidelity_tests`。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
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

#[cfg(test)]
mod wire_fidelity_tests {
    use super::*;

    /// 网关会把请求体反序列化成 AnthropicRequest 再**重新序列化**转发给上游
    /// （proxy.rs 的 `.json(&payload)`）。凡是这个结构体没有的字段，
    /// 都会在这一步被静默丢掉。
    ///
    /// 而 OMNIX 是把 agent CLI 指到自己网关上的（ANTHROPIC_BASE_URL=.../agent/<name>），
    /// 所以 Claude Code 的每一次带工具的请求都走这条路。
    #[test]
    fn tools_and_tool_blocks_survive_the_gateway_round_trip() {
        let raw = serde_json::json!({
            "model": "claude-x",
            "max_tokens": 100,
            "tools": [{
                "name": "Read",
                "description": "读文件",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }],
            "tool_choice": {"type": "auto"},
            "messages": [
                { "role": "user", "content": "看一下 a.txt" },
                { "role": "assistant", "content": [
                    {"type": "tool_use", "id": "tu_1", "name": "Read", "input": {"path": "a.txt"}}
                ]},
                { "role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1", "content": "文件内容"}
                ]}
            ]
        });

        let parsed: AnthropicRequest =
            serde_json::from_value(raw.clone()).expect("网关能解析这种请求");
        let forwarded = serde_json::to_value(&parsed).expect("重新序列化");

        assert!(
            forwarded.get("tools").is_some(),
            "工具定义被网关丢掉了——上游模型永远不知道有哪些工具可用，也就永远不会发起工具调用。\n转发出去的是：{forwarded}"
        );
        let tu = &forwarded["messages"][1]["content"][0];
        assert_eq!(tu["name"], "Read", "tool_use 的 name 丢了：{tu}");
        assert_eq!(tu["id"], "tu_1", "tool_use 的 id 丢了：{tu}");
        assert!(tu["input"].is_object(), "tool_use 的 input 丢了：{tu}");
        let tr = &forwarded["messages"][2]["content"][0];
        assert_eq!(tr["tool_use_id"], "tu_1", "tool_result 的关联 id 丢了：{tr}");
        assert_eq!(forwarded["tool_choice"]["type"], "auto");
    }

    /// 修的是这一**类**问题，不是 tools 一个字段。任何我们没写进结构体的东西
    /// 都必须原样穿过去——下一个新特性（thinking、citations、cache_control…）
    /// 不该再断一次工具链。
    #[test]
    fn unknown_future_fields_pass_through_untouched() {
        let raw = serde_json::json!({
            "model": "m", "max_tokens": 8,
            "messages": [{ "role": "user", "content": [
                {"type": "某种还没出现的块", "任意字段": {"嵌套": [1, 2]}, "cache_control": {"type": "ephemeral"}}
            ]}],
            "某个未来的顶层参数": {"a": 1},
            "metadata": {"user_id": "u1"}
        });
        let parsed: AnthropicRequest = serde_json::from_value(raw.clone()).unwrap();
        let out = serde_json::to_value(&parsed).unwrap();

        assert_eq!(out["某个未来的顶层参数"]["a"], 1, "顶层未知字段丢了：{out}");
        assert_eq!(out["metadata"]["user_id"], "u1");
        let blk = &out["messages"][0]["content"][0];
        assert_eq!(blk["任意字段"]["嵌套"][1], 2, "块内未知字段丢了：{blk}");
        assert_eq!(blk["cache_control"]["type"], "ephemeral");
    }

    /// 别往上游请求里塞 null。`{"type":"tool_use","text":null}` 正是原来那个
    /// bug 的现场——严格的上游会把它当畸形内容。
    #[test]
    fn absent_fields_are_omitted_not_sent_as_null() {
        let raw = serde_json::json!({
            "model": "m",
            "messages": [{ "role": "user", "content": [{"type": "tool_use", "name": "X", "input": {}}] }]
        });
        let parsed: AnthropicRequest = serde_json::from_value(raw).unwrap();
        let out = serde_json::to_string(&parsed).unwrap();
        assert!(!out.contains("null"), "转发的请求里不该出现 null：{out}");
        assert!(out.contains("\"name\":\"X\""));
    }
}
