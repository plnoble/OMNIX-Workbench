//! Translate between OpenAI's **Responses API** (what Codex speaks) and the
//! **Chat Completions API** (what most third-party providers speak).
//!
//! Codex 0.139 removed Chat Completions support and only emits Responses API
//! requests (`wire_api = "responses"`). Many providers a user would set as
//! their default model — DeepSeek, Volcano/doubao, most OpenAI-compatible
//! relays — only implement Chat Completions. The OMNIX session gateway uses
//! this module to accept Codex's Responses request, forward an equivalent Chat
//! Completions request upstream, and translate the streamed Chat Completions
//! response back into the Responses SSE events Codex understands.
//!
//! The request shape handled here was captured from a live
//! `codex app-server` turn (see `fixtures/codex_responses_request.json`).

use serde_json::{json, Value};

/// Convert a Codex Responses request body into a Chat Completions request body.
///
/// `model` overrides the request model with the upstream provider's model name.
pub fn responses_request_to_chat(responses: &Value, model: &str) -> Value {
    let mut messages: Vec<Value> = Vec::new();

    // `instructions` is the Responses system prompt; Chat carries it as system.
    if let Some(instructions) = responses
        .get("instructions")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        messages.push(json!({ "role": "system", "content": instructions }));
    }

    match responses.get("input") {
        Some(Value::Array(items)) => {
            for item in items {
                translate_input_item(item, &mut messages);
            }
        }
        Some(Value::String(text)) => {
            messages.push(json!({ "role": "user", "content": text }));
        }
        _ => {}
    }

    let stream = responses
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut chat = json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });
    let object = chat.as_object_mut().expect("chat object");

    if stream {
        // Ask Chat providers to include a usage block in the final chunk so we
        // can forward token accounting in `response.completed`.
        object.insert("stream_options".into(), json!({ "include_usage": true }));
    }

    if let Some(tools) = responses.get("tools").and_then(Value::as_array) {
        let chat_tools: Vec<Value> = tools.iter().filter_map(translate_tool).collect();
        if !chat_tools.is_empty() {
            object.insert("tools".into(), json!(chat_tools));
            if let Some(choice) = responses.get("tool_choice") {
                object.insert("tool_choice".into(), choice.clone());
            }
            if let Some(parallel) = responses.get("parallel_tool_calls") {
                object.insert("parallel_tool_calls".into(), parallel.clone());
            }
        }
    }

    if let Some(temperature) = responses.get("temperature") {
        object.insert("temperature".into(), temperature.clone());
    }
    if let Some(max_tokens) = responses.get("max_output_tokens") {
        object.insert("max_tokens".into(), max_tokens.clone());
    }

    chat
}

fn translate_input_item(item: &Value, messages: &mut Vec<Value>) {
    match item.get("type").and_then(Value::as_str).unwrap_or("message") {
        "message" => {
            let raw_role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            // Chat Completions has no `developer` role; fold it into `system`.
            let role = if raw_role == "developer" {
                "system"
            } else {
                raw_role
            };
            let text = collect_content_text(item.get("content"));
            messages.push(json!({ "role": role, "content": text }));
        }
        "function_call" => {
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = item
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            messages.push(json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": { "name": name, "arguments": arguments },
                }],
            }));
        }
        "function_call_output" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let content = match item.get("output") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": content,
            }));
        }
        _ => {}
    }
}

fn collect_content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn translate_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    let name = tool.get("name").and_then(Value::as_str)?;
    let mut function = json!({ "name": name });
    let function_obj = function.as_object_mut().expect("function object");
    if let Some(description) = tool.get("description") {
        function_obj.insert("description".into(), description.clone());
    }
    if let Some(parameters) = tool.get("parameters") {
        function_obj.insert("parameters".into(), parameters.clone());
    }
    Some(json!({ "type": "function", "function": function }))
}

#[derive(Default)]
struct MessageAccumulator {
    item_id: String,
    output_index: usize,
    text: String,
    started: bool,
}

struct ToolAccumulator {
    item_id: String,
    output_index: usize,
    call_id: String,
    name: String,
    arguments: String,
}

/// Stateful translator turning streamed Chat Completions chunks into the
/// Responses SSE event sequence Codex consumes.
pub struct ResponsesStreamTranslator {
    response_id: String,
    created: bool,
    sequence: u64,
    message: Option<MessageAccumulator>,
    tools: Vec<ToolAccumulator>,
    usage: Option<Value>,
    completed: bool,
}

impl ResponsesStreamTranslator {
    pub fn new(response_id: impl Into<String>) -> Self {
        Self {
            response_id: response_id.into(),
            created: false,
            sequence: 0,
            message: None,
            tools: Vec::new(),
            usage: None,
            completed: false,
        }
    }

    fn event(&mut self, kind: &str, mut data: Value) -> String {
        if let Some(object) = data.as_object_mut() {
            object.insert("type".into(), json!(kind));
            object.insert("sequence_number".into(), json!(self.sequence));
        }
        self.sequence += 1;
        format!(
            "event: {kind}\ndata: {}\n\n",
            serde_json::to_string(&data).unwrap_or_else(|_| "{}".into())
        )
    }

    fn ensure_created(&mut self, events: &mut Vec<String>) {
        if self.created {
            return;
        }
        self.created = true;
        let response_id = self.response_id.clone();
        events.push(self.event(
            "response.created",
            json!({
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                    "output": [],
                }
            }),
        ));
    }

    /// Process one Chat Completions streaming chunk (`chat.completion.chunk`).
    pub fn push_chunk(&mut self, chunk: &Value) -> Vec<String> {
        let mut events = Vec::new();
        self.ensure_created(&mut events);

        if let Some(usage) = chunk.get("usage").filter(|usage| !usage.is_null()) {
            self.usage = Some(map_usage(usage));
        }

        let Some(delta) = chunk.pointer("/choices/0/delta") else {
            return events;
        };

        if let Some(text) = delta
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            self.push_text_delta(text, &mut events);
        }

        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in tool_calls {
                self.push_tool_delta(call, &mut events);
            }
        }

        events
    }

    fn push_text_delta(&mut self, text: &str, events: &mut Vec<String>) {
        if self.message.is_none() {
            let output_index = self.next_output_index();
            let item_id = format!("msg_{}", self.response_id);
            self.message = Some(MessageAccumulator {
                item_id: item_id.clone(),
                output_index,
                text: String::new(),
                started: true,
            });
            events.push(self.event(
                "response.output_item.added",
                json!({
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "message",
                        "role": "assistant",
                        "status": "in_progress",
                        "content": [],
                    }
                }),
            ));
            events.push(self.event(
                "response.content_part.added",
                json!({
                    "item_id": item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "part": { "type": "output_text", "text": "" },
                }),
            ));
        }

        let (item_id, output_index) = {
            let message = self.message.as_mut().expect("message accumulator");
            message.text.push_str(text);
            (message.item_id.clone(), message.output_index)
        };
        events.push(self.event(
            "response.output_text.delta",
            json!({
                "item_id": item_id,
                "output_index": output_index,
                "content_index": 0,
                "delta": text,
            }),
        ));
    }

    fn push_tool_delta(&mut self, call: &Value, events: &mut Vec<String>) {
        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        if self.tools.get(index).is_none() {
            let output_index = self.next_output_index();
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("call_{index}"));
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let item_id = format!("fc_{}_{index}", self.response_id);
            while self.tools.len() <= index {
                self.tools.push(ToolAccumulator {
                    item_id: item_id.clone(),
                    output_index,
                    call_id: call_id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                });
            }
            events.push(self.event(
                "response.output_item.added",
                json!({
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "function_call",
                        "status": "in_progress",
                        "name": name,
                        "call_id": call_id,
                        "arguments": "",
                    }
                }),
            ));
        }

        // Late-arriving id/name fields.
        if let Some(id) = call.get("id").and_then(Value::as_str) {
            if let Some(tool) = self.tools.get_mut(index) {
                if tool.call_id.starts_with("call_") {
                    tool.call_id = id.to_string();
                }
            }
        }
        if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
            if let Some(tool) = self.tools.get_mut(index) {
                if tool.name.is_empty() {
                    tool.name = name.to_string();
                }
            }
        }

        if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str) {
            if !arguments.is_empty() {
                let (item_id, output_index) = {
                    let tool = self.tools.get_mut(index).expect("tool accumulator");
                    tool.arguments.push_str(arguments);
                    (tool.item_id.clone(), tool.output_index)
                };
                events.push(self.event(
                    "response.function_call_arguments.delta",
                    json!({
                        "item_id": item_id,
                        "output_index": output_index,
                        "delta": arguments,
                    }),
                ));
            }
        }
    }

    fn next_output_index(&self) -> usize {
        self.message.iter().count() + self.tools.len()
    }

    /// Emit closing events and `response.completed`. Idempotent.
    pub fn finish(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if self.completed {
            return events;
        }
        self.ensure_created(&mut events);
        self.completed = true;

        let mut output = Vec::new();

        if let Some(message) = self.message.take() {
            if message.started {
                events.push(self.event(
                    "response.output_text.done",
                    json!({
                        "item_id": message.item_id,
                        "output_index": message.output_index,
                        "content_index": 0,
                        "text": message.text,
                    }),
                ));
                events.push(self.event(
                    "response.content_part.done",
                    json!({
                        "item_id": message.item_id,
                        "output_index": message.output_index,
                        "content_index": 0,
                        "part": { "type": "output_text", "text": message.text },
                    }),
                ));
                let item = json!({
                    "id": message.item_id,
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{ "type": "output_text", "text": message.text }],
                });
                events.push(self.event(
                    "response.output_item.done",
                    json!({ "output_index": message.output_index, "item": item.clone() }),
                ));
                output.push(item);
            }
        }

        let tools = std::mem::take(&mut self.tools);
        for tool in tools {
            events.push(self.event(
                "response.function_call_arguments.done",
                json!({
                    "item_id": tool.item_id,
                    "output_index": tool.output_index,
                    "arguments": tool.arguments,
                }),
            ));
            let item = json!({
                "id": tool.item_id,
                "type": "function_call",
                "status": "completed",
                "name": tool.name,
                "call_id": tool.call_id,
                "arguments": tool.arguments,
            });
            events.push(self.event(
                "response.output_item.done",
                json!({ "output_index": tool.output_index, "item": item.clone() }),
            ));
            output.push(item);
        }

        let usage = self.usage.clone().unwrap_or(Value::Null);
        let response_id = self.response_id.clone();
        events.push(self.event(
            "response.completed",
            json!({
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "completed",
                    "output": output,
                    "usage": usage,
                }
            }),
        ));
        events
    }

    /// Translate a complete (non-streamed) Chat Completions response into the
    /// full Responses SSE sequence. Used when an upstream ignores `stream`.
    pub fn translate_full(&mut self, completion: &Value) -> Vec<String> {
        let mut events = Vec::new();
        self.ensure_created(&mut events);

        if let Some(usage) = completion.get("usage").filter(|usage| !usage.is_null()) {
            self.usage = Some(map_usage(usage));
        }

        let message = completion.pointer("/choices/0/message");
        if let Some(text) = message
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            events.extend(self.push_text_chunk(text));
        }
        if let Some(tool_calls) = message
            .and_then(|message| message.get("tool_calls"))
            .and_then(Value::as_array)
        {
            for (index, call) in tool_calls.iter().enumerate() {
                let mut streamed = call.clone();
                if let Some(object) = streamed.as_object_mut() {
                    object.insert("index".into(), json!(index));
                }
                events.extend(self.push_tool_chunk(&streamed));
            }
        }

        events.extend(self.finish());
        events
    }

    fn push_text_chunk(&mut self, text: &str) -> Vec<String> {
        let mut events = Vec::new();
        self.push_text_delta(text, &mut events);
        events
    }

    fn push_tool_chunk(&mut self, call: &Value) -> Vec<String> {
        let mut events = Vec::new();
        self.push_tool_delta(call, &mut events);
        events
    }
}

fn map_usage(usage: &Value) -> Value {
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(input + output);
    json!({ "input_tokens": input, "output_tokens": output, "total_tokens": total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_request_becomes_chat_with_system_and_messages() {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/codex_responses_request.json"))
                .expect("fixture parses");
        let chat = responses_request_to_chat(&fixture, "doubao-pro");

        assert_eq!(chat["model"], "doubao-pro");
        assert_eq!(chat["stream"], true);
        let messages = chat["messages"].as_array().expect("messages");
        // instructions -> system, developer -> system, user -> user
        assert_eq!(messages[0]["role"], "system");
        assert!(messages[0]["content"].as_str().unwrap().contains("Codex"));
        assert_eq!(messages[1]["role"], "system"); // developer folded into system
        assert_eq!(messages.last().unwrap()["role"], "user");
        assert_eq!(messages.last().unwrap()["content"], "say hi");

        let tools = chat["tools"].as_array().expect("tools");
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "shell_command");
        assert!(tools[0]["function"]["parameters"].is_object());
        assert_eq!(chat["tool_choice"], "auto");
    }

    #[test]
    fn function_call_and_output_items_become_chat_tool_turns() {
        let responses = json!({
            "input": [
                { "type": "function_call", "call_id": "c1", "name": "shell", "arguments": "{\"cmd\":\"ls\"}" },
                { "type": "function_call_output", "call_id": "c1", "output": "file.txt" },
            ]
        });
        let chat = responses_request_to_chat(&responses, "m");
        let messages = chat["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "c1");
        assert_eq!(messages[0]["tool_calls"][0]["function"]["name"], "shell");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "c1");
        assert_eq!(messages[1]["content"], "file.txt");
    }

    fn joined(events: &[String]) -> String {
        events.join("")
    }

    #[test]
    fn streamed_text_produces_ordered_responses_events() {
        let mut translator = ResponsesStreamTranslator::new("resp_test");
        let mut all = Vec::new();
        all.extend(translator.push_chunk(&json!({
            "choices": [{ "delta": { "content": "Hel" } }]
        })));
        all.extend(translator.push_chunk(&json!({
            "choices": [{ "delta": { "content": "lo" } }]
        })));
        all.extend(translator.push_chunk(&json!({
            "choices": [{ "delta": {} }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
        })));
        all.extend(translator.finish());
        let text = joined(&all);

        assert!(text.contains("event: response.created"));
        assert!(text.contains("event: response.output_item.added"));
        assert!(text.contains("event: response.content_part.added"));
        assert!(text.contains("event: response.output_text.delta"));
        assert!(text.contains("event: response.output_text.done"));
        assert!(text.contains("event: response.completed"));
        // Concatenated text is preserved.
        assert!(text.contains("\"text\":\"Hello\""));
        // Usage is mapped to Responses naming.
        assert!(text.contains("\"input_tokens\":5"));
        assert!(text.contains("\"output_tokens\":2"));
        // finish is idempotent.
        assert!(translator.finish().is_empty());
    }

    #[test]
    fn streamed_tool_call_produces_function_call_events() {
        let mut translator = ResponsesStreamTranslator::new("resp_tool");
        let mut all = Vec::new();
        all.extend(translator.push_chunk(&json!({
            "choices": [{ "delta": { "tool_calls": [{
                "index": 0, "id": "call_abc",
                "function": { "name": "shell", "arguments": "{\"cmd\"" }
            }] } }]
        })));
        all.extend(translator.push_chunk(&json!({
            "choices": [{ "delta": { "tool_calls": [{
                "index": 0, "function": { "arguments": ":\"ls\"}" }
            }] } }]
        })));
        all.extend(translator.finish());
        let text = joined(&all);

        assert!(text.contains("\"type\":\"function_call\""));
        assert!(text.contains("event: response.function_call_arguments.delta"));
        assert!(text.contains("event: response.function_call_arguments.done"));
        assert!(text.contains("\"call_id\":\"call_abc\""));
        assert!(text.contains("\"name\":\"shell\""));
        assert!(text.contains("{\\\"cmd\\\":\\\"ls\\\"}"));
    }

    #[test]
    fn non_streaming_completion_translates_to_full_sequence() {
        let mut translator = ResponsesStreamTranslator::new("resp_full");
        let events = translator.translate_full(&json!({
            "choices": [{ "message": { "role": "assistant", "content": "done" } }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1 }
        }));
        let text = joined(&events);
        assert!(text.contains("event: response.created"));
        assert!(text.contains("\"text\":\"done\""));
        assert!(text.contains("event: response.completed"));
    }
}
