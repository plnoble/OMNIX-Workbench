//! Anthropic ⇄ OpenAI 工具调用翻译。
//!
//! ## 为什么非做不可
//!
//! OMNIX 把 agent CLI 指到自己的网关（`ANTHROPIC_BASE_URL=.../agent/<名字>`），
//! 所以 Claude Code 说的是 Anthropic 协议。当用户给这个 agent 绑了一个
//! OpenAI 兼容的上游时，网关要现场翻译。原来这条路径**只翻译文本**：工具定义
//! 整个丢掉，上游模型根本不知道有工具，agent 于是一个工具都调不了。
//!
//! 之前留了个 `log::warn!` 占位，并写明「不做只翻译请求不翻译响应的半吊子版本
//! ——那样模型会返回 tool_calls 而我们没有反向翻译，比它压根不知道有工具更糟」。
//! 这个模块就是把三个方向一次做全：
//!
//! 1. 请求：`tools` / `tool_use` / `tool_result` → OpenAI 的 `tools` /
//!    `tool_calls` / `role:"tool"`
//! 2. 非流式响应：OpenAI `tool_calls` → Anthropic `tool_use` 块
//! 3. 流式响应：OpenAI 分片的 `delta.tool_calls[]` → Anthropic 的
//!    `content_block_start` / `input_json_delta` / `content_block_stop`
//!
//! ## 为什么全是纯函数
//!
//! 本机没有 OpenAI 兼容上游可以打，端到端验不了。所以协议逻辑一点都不留在
//! handler 里——全部做成「JSON 进、JSON 出」的纯函数，用真实报文形状喂测试。
//! proxy.rs 那边只剩接线。
//!
//! ## 两个容易翻车的点
//!
//! - OpenAI 的 `function.arguments` 是**一个 JSON 字符串**，不是对象；
//!   Anthropic 的 `input` 是对象。两个方向都要转，且流式时字符串是分片来的。
//! - Anthropic 的 content block 索引必须连续且唯一，跨 text 与 tool_use 统一编号。
//!   原来的实现把每个文本增量都写死 `index: 0`，且从不发
//!   `content_block_start`/`stop`——本来就不是一个合规的 Anthropic 流。

use serde_json::{json, Map, Value};

// ── 请求方向：Anthropic → OpenAI ────────────────

/// Anthropic 的工具定义 → OpenAI 的 `tools`。
///
/// `input_schema` → `function.parameters`；其余原样。认不出形状的条目直接跳过，
/// 而不是塞个残缺定义给上游——上游多半会整个请求 400，那比少一个工具更糟。
pub fn tools_to_openai(anthropic_tools: &Value) -> Option<Value> {
    let arr = anthropic_tools.as_array()?;
    let out: Vec<Value> = arr
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?;
            let mut function = Map::new();
            function.insert("name".into(), json!(name));
            if let Some(desc) = t.get("description") {
                function.insert("description".into(), desc.clone());
            }
            // 没给 schema 的工具也是合法的（无参工具），补一个空 object，
            // 因为部分上游要求 parameters 必须在。
            let params = t
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            function.insert("parameters".into(), params);
            Some(json!({ "type": "function", "function": Value::Object(function) }))
        })
        .collect();
    (!out.is_empty()).then(|| Value::Array(out))
}

/// Anthropic 的 `tool_choice` → OpenAI 的 `tool_choice`。
pub fn tool_choice_to_openai(choice: &Value) -> Option<Value> {
    // 有些客户端直接传字符串，原样透传。
    if choice.is_string() {
        return Some(choice.clone());
    }
    match choice.get("type")?.as_str()? {
        "auto" => Some(json!("auto")),
        // Anthropic 的 any = 必须调某个工具 = OpenAI 的 required。
        "any" => Some(json!("required")),
        "none" => Some(json!("none")),
        "tool" => {
            let name = choice.get("name")?.as_str()?;
            Some(json!({ "type": "function", "function": { "name": name } }))
        }
        _ => None,
    }
}

/// `tool_result` 的 content 可能是字符串，也可能是块数组。
/// OpenAI 的 tool 消息只收字符串，所以这里压平。
fn tool_result_text(content: Option<&Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => b.get("text").and_then(|t| t.as_str()).map(str::to_string),
                // 图片等非文本结果在 OpenAI 的 tool 消息里没有位置，
                // 留个可见的占位比静默丢掉强。
                Some(other) => Some(format!("[{other}]")),
                None => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
    }
}

/// 把一条 Anthropic 消息展开成一条或多条 OpenAI 消息。
///
/// 会变多是因为 `tool_result` 必须各自成为独立的 `role:"tool"` 消息，
/// 且**必须紧跟在**请求它的那条 assistant 消息之后——所以顺序不能乱。
fn message_to_openai(role: &str, content: &Value, out: &mut Vec<Value>) {
    let Some(blocks) = content.as_array() else {
        // 纯字符串内容：老路径，原样。
        out.push(json!({ "role": role, "content": content.clone() }));
        return;
    };

    let mut text_parts: Vec<String> = Vec::new();
    let mut image_parts: Vec<Value> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    let mut tool_results: Vec<Value> = Vec::new();

    for block in blocks {
        match block.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "text" => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(t.to_string());
                }
            }
            "image" => {
                if let Some(src) = block.get("source") {
                    let media = src.get("media_type").and_then(|m| m.as_str()).unwrap_or("image/png");
                    if let Some(data) = src.get("data").and_then(|d| d.as_str()) {
                        image_parts.push(json!({
                            "type": "image_url",
                            "image_url": { "url": format!("data:{media};base64,{data}") }
                        }));
                    } else if let Some(url) = src.get("url").and_then(|u| u.as_str()) {
                        image_parts
                            .push(json!({"type": "image_url", "image_url": {"url": url}}));
                    }
                }
            }
            "tool_use" => {
                let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                // OpenAI 的 arguments 是 JSON **字符串**，不是对象。
                let args = block
                    .get("input")
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "{}".into());
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": args }
                }));
            }
            "tool_result" => {
                let id = block.get("tool_use_id").and_then(|i| i.as_str()).unwrap_or("");
                tool_results.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": tool_result_text(block.get("content")),
                }));
            }
            _ => {}
        }
    }

    // tool_result 先出：它回答的是上一条 assistant，必须紧跟其后。
    out.extend(tool_results);

    let joined = text_parts.join("\n");
    if !tool_calls.is_empty() {
        let mut msg = Map::new();
        msg.insert("role".into(), json!("assistant"));
        // 带 tool_calls 时 content 允许为 null，但不少上游更认空串。
        msg.insert("content".into(), json!(joined));
        msg.insert("tool_calls".into(), Value::Array(tool_calls));
        out.push(Value::Object(msg));
    } else if !image_parts.is_empty() {
        let mut parts = Vec::new();
        if !joined.is_empty() {
            parts.push(json!({"type": "text", "text": joined}));
        }
        parts.extend(image_parts);
        out.push(json!({ "role": role, "content": Value::Array(parts) }));
    } else if !joined.is_empty() {
        out.push(json!({ "role": role, "content": joined }));
    }
    // 全是 tool_result 的那条 user 消息不再额外产生一条空 user 消息——
    // 空消息会被部分上游判为非法。
}

/// 把整份 Anthropic 消息列表翻成 OpenAI 的 `messages`。
pub fn messages_to_openai(system: Option<&Value>, messages: &[(String, Value)]) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(sys) = system {
        let text = match sys {
            Value::String(s) => s.clone(),
            Value::Array(_) => tool_result_text(Some(sys)),
            other => other.to_string(),
        };
        if !text.is_empty() {
            out.push(json!({ "role": "system", "content": text }));
        }
    }
    for (role, content) in messages {
        message_to_openai(role, content, &mut out);
    }
    out
}

// ── 响应方向：OpenAI → Anthropic ────────────────

/// OpenAI 的 `finish_reason` → Anthropic 的 `stop_reason`。
pub fn stop_reason_to_anthropic(finish: Option<&str>) -> &'static str {
    match finish {
        Some("tool_calls") | Some("function_call") => "tool_use",
        Some("length") => "max_tokens",
        // content_filter 在 Anthropic 侧没有对应值；按正常收尾报，
        // 免得客户端撞上不认识的 stop_reason。
        _ => "end_turn",
    }
}

/// `arguments` 字符串 → Anthropic 的 `input` 对象。
///
/// 解析不了就给空对象并留一条日志。这里**不能**报错中断：一次工具参数畸形
/// 不该让整个响应无法返回，客户端拿到空参数至少能看到模型试图调用什么。
fn arguments_to_input(arguments: Option<&str>) -> Value {
    let raw = arguments.unwrap_or("").trim();
    if raw.is_empty() {
        return json!({});
    }
    match serde_json::from_str::<Value>(raw) {
        Ok(v) if v.is_object() => v,
        _ => {
            log::warn!("网关：上游返回的工具参数不是合法 JSON 对象，按空参数处理：{raw}");
            json!({})
        }
    }
}

/// 非流式：OpenAI 的 `choices[0].message` → Anthropic 的 content 块数组。
pub fn content_blocks_from_openai_message(message: &Value) -> Vec<Value> {
    let mut blocks = Vec::new();
    if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            blocks.push(json!({ "type": "text", "text": text }));
        }
    }
    if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let function = call.get("function");
            blocks.push(json!({
                "type": "tool_use",
                "id": call.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                "name": function.and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or(""),
                "input": arguments_to_input(
                    function.and_then(|f| f.get("arguments")).and_then(|a| a.as_str())
                ),
            }));
        }
    }
    // Anthropic 的 content 不能是空数组。
    if blocks.is_empty() {
        blocks.push(json!({ "type": "text", "text": "" }));
    }
    blocks
}

// ── 流式：OpenAI SSE → Anthropic SSE ────────────

fn sse(event: &str, data: &Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

/// 正在累积中的一个工具调用。
struct ToolSlot {
    /// 分配给它的 Anthropic content block 索引。`None` = 还没拿到 name，
    /// 尚未发出 `content_block_start`。
    block_index: Option<usize>,
    id: String,
    name: String,
    /// 在 `content_block_start` 发出之前抢先到达的参数分片。
    pending_args: String,
}

/// OpenAI 流 → Anthropic 流的转换器。
///
/// 有状态是因为 OpenAI 把工具参数**逐字符分片**发送（`{"pa` / `th":"a"}`），
/// 而 Anthropic 侧要求先 `content_block_start` 再一串 `input_json_delta`。
/// 分片还可能被 TCP 切在任意位置，所以行缓冲也在这里。
pub struct StreamTranslator {
    line_buffer: Vec<u8>,
    started: bool,
    model: String,
    next_block: usize,
    text_block: Option<usize>,
    /// 按 OpenAI 的 `tool_calls[].index` 归位。
    slots: Vec<Option<ToolSlot>>,
    finish_reason: Option<String>,
    finished: bool,
}

impl StreamTranslator {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            line_buffer: Vec::new(),
            started: false,
            model: model.into(),
            next_block: 0,
            text_block: None,
            slots: Vec::new(),
            finish_reason: None,
            finished: false,
        }
    }

    /// 喂一段上游原始字节，返回要写给下游的 Anthropic SSE 文本。
    pub fn push_bytes(&mut self, chunk: &[u8]) -> String {
        self.line_buffer.extend_from_slice(chunk);
        let mut out = String::new();
        while let Some(pos) = self.line_buffer.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&self.line_buffer[..pos]).trim().to_string();
            self.line_buffer.drain(..pos + 1);
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload == "[DONE]" {
                out.push_str(&self.finish());
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<Value>(payload) {
                out.push_str(&self.push_chunk(&chunk));
            }
        }
        out
    }

    fn ensure_started(&mut self) -> String {
        if self.started {
            return String::new();
        }
        self.started = true;
        sse(
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": "msg_omnix_stream",
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "stop_reason": Value::Null,
                    "stop_sequence": Value::Null,
                    "usage": { "input_tokens": 0, "output_tokens": 0 }
                }
            }),
        )
    }

    fn close_text_block(&mut self) -> String {
        match self.text_block.take() {
            Some(i) => sse("content_block_stop", &json!({"type": "content_block_stop", "index": i})),
            None => String::new(),
        }
    }

    /// 处理一个已解析的 OpenAI chunk。
    pub fn push_chunk(&mut self, chunk: &Value) -> String {
        let mut out = self.ensure_started();
        let Some(choice) = chunk.get("choices").and_then(|c| c.as_array()).and_then(|c| c.first())
        else {
            return out;
        };
        if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
            self.finish_reason = Some(reason.to_string());
        }
        let Some(delta) = choice.get("delta") else {
            return out;
        };

        if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                if self.text_block.is_none() {
                    let index = self.next_block;
                    self.next_block += 1;
                    self.text_block = Some(index);
                    out.push_str(&sse(
                        "content_block_start",
                        &json!({
                            "type": "content_block_start",
                            "index": index,
                            "content_block": {"type": "text", "text": ""}
                        }),
                    ));
                }
                out.push_str(&sse(
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta",
                        "index": self.text_block.unwrap_or(0),
                        "delta": {"type": "text_delta", "text": text}
                    }),
                ));
            }
        }

        if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
            // 工具块开始前必须先把文本块收掉：Anthropic 的块不能交错。
            if !calls.is_empty() {
                out.push_str(&self.close_text_block());
            }
            for call in calls {
                out.push_str(&self.push_tool_fragment(call));
            }
        }
        out
    }

    fn push_tool_fragment(&mut self, call: &Value) -> String {
        let idx = call.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
        // 上游给的 index 不受我们控制，别让它决定分配多大内存。
        if idx > 256 {
            return String::new();
        }
        while self.slots.len() <= idx {
            self.slots.push(None);
        }
        let slot = self.slots[idx].get_or_insert_with(|| ToolSlot {
            block_index: None,
            id: String::new(),
            name: String::new(),
            pending_args: String::new(),
        });
        if let Some(id) = call.get("id").and_then(|i| i.as_str()) {
            if !id.is_empty() {
                slot.id = id.to_string();
            }
        }
        let function = call.get("function");
        if let Some(name) = function.and_then(|f| f.get("name")).and_then(|n| n.as_str()) {
            if !name.is_empty() {
                slot.name.push_str(name);
            }
        }
        let args = function
            .and_then(|f| f.get("arguments"))
            .and_then(|a| a.as_str())
            .unwrap_or("");

        let mut out = String::new();
        if slot.block_index.is_none() {
            // 名字还没到就先攒着——发一个没有 name 的 tool_use 客户端没法用。
            if slot.name.is_empty() {
                slot.pending_args.push_str(args);
                return out;
            }
            let index = self.next_block;
            self.next_block += 1;
            let slot = self.slots[idx].as_mut().expect("刚建好");
            slot.block_index = Some(index);
            let (id, name) = (slot.id.clone(), slot.name.clone());
            let buffered = std::mem::take(&mut slot.pending_args);
            out.push_str(&sse(
                "content_block_start",
                &json!({
                    "type": "content_block_start",
                    "index": index,
                    "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
                }),
            ));
            if !buffered.is_empty() {
                out.push_str(&sse(
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {"type": "input_json_delta", "partial_json": buffered}
                    }),
                ));
            }
        }
        if !args.is_empty() {
            let index = self.slots[idx].as_ref().and_then(|s| s.block_index).unwrap_or(0);
            out.push_str(&sse(
                "content_block_delta",
                &json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {"type": "input_json_delta", "partial_json": args}
                }),
            ));
        }
        out
    }

    /// 收尾：关掉所有还开着的块，再发 `message_delta` + `message_stop`。
    ///
    /// 幂等——`[DONE]` 和流自然结束都可能触发，发两遍会让客户端看到两个
    /// message_stop。
    pub fn finish(&mut self) -> String {
        if self.finished {
            return String::new();
        }
        self.finished = true;
        let mut out = self.ensure_started();
        out.push_str(&self.close_text_block());
        for slot in self.slots.iter().flatten() {
            if let Some(index) = slot.block_index {
                out.push_str(&sse(
                    "content_block_stop",
                    &json!({"type": "content_block_stop", "index": index}),
                ));
            }
        }
        let stop = stop_reason_to_anthropic(self.finish_reason.as_deref());
        out.push_str(&sse(
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop, "stop_sequence": Value::Null},
                "usage": {"output_tokens": 0}
            }),
        ));
        out.push_str(&sse("message_stop", &json!({"type": "message_stop"})));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 请求方向 ──

    #[test]
    fn tool_definitions_become_openai_functions() {
        let tools = json!([{
            "name": "Read",
            "description": "读文件",
            "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
        }]);
        let out = tools_to_openai(&tools).expect("应当翻出来");
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["function"]["name"], "Read");
        assert_eq!(out[0]["function"]["description"], "读文件");
        // input_schema → parameters，是这套翻译最容易漏的一处改名。
        assert_eq!(out[0]["function"]["parameters"]["properties"]["path"]["type"], "string");
    }

    #[test]
    fn malformed_tool_entries_are_skipped_not_forwarded_broken() {
        // 少了 name 的条目转过去多半让上游把整个请求 400——
        // 那比少一个工具糟得多。
        let out = tools_to_openai(&json!([{"description": "没有名字"}, {"name": "Ok"}]));
        let out = out.expect("至少剩一个");
        assert_eq!(out.as_array().unwrap().len(), 1);
        assert_eq!(out[0]["function"]["name"], "Ok");
        // 无参工具也得有 parameters，部分上游强制要求。
        assert_eq!(out[0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn tool_choice_modes_map_across() {
        assert_eq!(tool_choice_to_openai(&json!({"type": "auto"})).unwrap(), json!("auto"));
        // Anthropic 的 any（必须调工具）= OpenAI 的 required，不是 auto。
        assert_eq!(tool_choice_to_openai(&json!({"type": "any"})).unwrap(), json!("required"));
        assert_eq!(tool_choice_to_openai(&json!({"type": "none"})).unwrap(), json!("none"));
        let forced = tool_choice_to_openai(&json!({"type": "tool", "name": "Read"})).unwrap();
        assert_eq!(forced["function"]["name"], "Read");
        assert_eq!(forced["type"], "function");
    }

    #[test]
    fn tool_use_block_becomes_tool_calls_with_stringified_arguments() {
        let msgs = vec![(
            "assistant".to_string(),
            json!([{"type": "tool_use", "id": "tu_1", "name": "Read", "input": {"path": "a.txt"}}]),
        )];
        let out = messages_to_openai(None, &msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["role"], "assistant");
        let call = &out[0]["tool_calls"][0];
        assert_eq!(call["id"], "tu_1");
        assert_eq!(call["function"]["name"], "Read");
        // 这是最容易翻车的一处：OpenAI 的 arguments 是**字符串**不是对象。
        let args = call["function"]["arguments"].as_str().expect("arguments 必须是字符串");
        assert_eq!(serde_json::from_str::<Value>(args).unwrap()["path"], "a.txt");
    }

    #[test]
    fn tool_result_becomes_its_own_tool_role_message() {
        let msgs = vec![
            (
                "assistant".to_string(),
                json!([{"type": "tool_use", "id": "tu_1", "name": "Read", "input": {}}]),
            ),
            (
                "user".to_string(),
                json!([{"type": "tool_result", "tool_use_id": "tu_1", "content": "文件内容"}]),
            ),
        ];
        let out = messages_to_openai(None, &msgs);
        assert_eq!(out.len(), 2, "tool_result 要独立成一条消息");
        assert_eq!(out[1]["role"], "tool");
        assert_eq!(out[1]["tool_call_id"], "tu_1", "对不上 id 上游会拒收整轮对话");
        assert_eq!(out[1]["content"], "文件内容");
        // 只有 tool_result 的那条 user 消息不该再多出一条空 user。
        assert!(out.iter().all(|m| m["role"] != "user"));
    }

    #[test]
    fn tool_results_precede_the_text_that_shares_their_message() {
        // Claude Code 常把 tool_result 和后续提问放在同一条 user 消息里。
        // tool 消息必须紧跟发起调用的 assistant，顺序错了上游会报
        // 「tool_call_id 没有对应的 tool_calls」。
        let msgs = vec![(
            "user".to_string(),
            json!([
                {"type": "tool_result", "tool_use_id": "tu_1", "content": "结果"},
                {"type": "text", "text": "接着做下一步"}
            ]),
        )];
        let out = messages_to_openai(None, &msgs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["role"], "tool");
        assert_eq!(out[1]["role"], "user");
        assert_eq!(out[1]["content"], "接着做下一步");
    }

    #[test]
    fn block_shaped_tool_result_content_is_flattened_to_text() {
        let msgs = vec![(
            "user".to_string(),
            json!([{
                "type": "tool_result", "tool_use_id": "t1",
                "content": [{"type": "text", "text": "第一段"}, {"type": "text", "text": "第二段"}]
            }]),
        )];
        let out = messages_to_openai(None, &msgs);
        assert_eq!(out[0]["content"], "第一段\n第二段");
    }

    #[test]
    fn plain_text_and_images_keep_their_old_shape() {
        // 回归保护：这条路径原来就能走通的东西不能被工具翻译改坏。
        let msgs = vec![
            ("user".to_string(), json!("你好")),
            (
                "user".to_string(),
                json!([
                    {"type": "text", "text": "看图"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAA"}}
                ]),
            ),
        ];
        let out = messages_to_openai(Some(&json!("你是助手")), &msgs);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[1]["content"], "你好");
        assert_eq!(out[2]["content"][0]["text"], "看图");
        assert_eq!(out[2]["content"][1]["image_url"]["url"], "data:image/png;base64,AAA");
    }

    // ── 非流式响应方向 ──

    #[test]
    fn openai_tool_calls_become_anthropic_tool_use_blocks() {
        let message = json!({
            "role": "assistant",
            "content": "我来读一下",
            "tool_calls": [{
                "id": "call_1", "type": "function",
                "function": {"name": "Read", "arguments": "{\"path\":\"a.txt\"}"}
            }]
        });
        let blocks = content_blocks_from_openai_message(&message);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["id"], "call_1");
        // arguments 字符串 → input 对象，方向和请求侧相反。
        assert_eq!(blocks[1]["input"]["path"], "a.txt");
    }

    #[test]
    fn malformed_arguments_yield_empty_input_not_a_broken_response() {
        let message = json!({
            "tool_calls": [{"id": "c", "function": {"name": "X", "arguments": "{不是 json"}}]
        });
        let blocks = content_blocks_from_openai_message(&message);
        assert_eq!(blocks[0]["input"], json!({}), "畸形参数不该让整个响应发不出去");
        assert_eq!(blocks[0]["name"], "X", "至少让用户看到模型想调什么");
    }

    #[test]
    fn empty_message_still_produces_a_content_block() {
        // Anthropic 的 content 不允许是空数组。
        let blocks = content_blocks_from_openai_message(&json!({"content": ""}));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
    }

    #[test]
    fn finish_reason_maps_to_stop_reason() {
        assert_eq!(stop_reason_to_anthropic(Some("tool_calls")), "tool_use");
        assert_eq!(stop_reason_to_anthropic(Some("length")), "max_tokens");
        assert_eq!(stop_reason_to_anthropic(Some("stop")), "end_turn");
        // 不认识的值不能原样透出去，客户端会撞上未知 stop_reason。
        assert_eq!(stop_reason_to_anthropic(Some("content_filter")), "end_turn");
        assert_eq!(stop_reason_to_anthropic(None), "end_turn");
    }

    // ── 流式方向 ──

    /// 把翻出来的 SSE 解析回事件序列，方便断言。
    fn events(sse_text: &str) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for block in sse_text.split("\n\n") {
            let mut name = None;
            let mut data = None;
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event: ") {
                    name = Some(v.to_string());
                } else if let Some(v) = line.strip_prefix("data: ") {
                    data = serde_json::from_str::<Value>(v).ok();
                }
            }
            if let (Some(n), Some(d)) = (name, data) {
                out.push((n, d));
            }
        }
        out
    }

    fn event_names(sse_text: &str) -> Vec<String> {
        events(sse_text).into_iter().map(|(n, _)| n).collect()
    }

    fn feed(t: &mut StreamTranslator, chunks: &[&str]) -> String {
        let mut out = String::new();
        for c in chunks {
            out.push_str(&t.push_bytes(format!("data: {c}\n").as_bytes()));
        }
        out
    }

    #[test]
    fn text_only_stream_is_a_well_formed_anthropic_stream() {
        // 原来的实现只发 content_block_delta，从不发 message_start /
        // content_block_start / stop——本来就不是合规的 Anthropic 流。
        let mut t = StreamTranslator::new("m");
        let mut sse_text = feed(
            &mut t,
            &[
                r#"{"choices":[{"delta":{"content":"你"}}]}"#,
                r#"{"choices":[{"delta":{"content":"好"}}]}"#,
                r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            ],
        );
        sse_text.push_str(&t.push_bytes(b"data: [DONE]\n"));
        assert_eq!(
            event_names(&sse_text),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop"
            ]
        );
    }

    #[test]
    fn fragmented_tool_arguments_become_input_json_deltas() {
        let mut t = StreamTranslator::new("m");
        let sse_text = feed(
            &mut t,
            &[
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"Read","arguments":""}}]}}]}"#,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pa"}}]}}]}"#,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"a.txt\"}"}}]}}]}"#,
            ],
        );
        let evts = events(&sse_text);
        let start = evts.iter().find(|(n, _)| n == "content_block_start").expect("要有 start");
        assert_eq!(start.1["content_block"]["type"], "tool_use");
        assert_eq!(start.1["content_block"]["id"], "call_1");
        assert_eq!(start.1["content_block"]["name"], "Read");

        // 分片拼起来必须还原成合法 JSON——这是整条链路的关键不变量。
        let joined: String = evts
            .iter()
            .filter(|(n, d)| n == "content_block_delta" && d["delta"]["type"] == "input_json_delta")
            .map(|(_, d)| d["delta"]["partial_json"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(serde_json::from_str::<Value>(&joined).unwrap()["path"], "a.txt");
    }

    #[test]
    fn text_block_is_closed_before_a_tool_block_opens() {
        // Anthropic 的 content block 不能交错。
        let mut t = StreamTranslator::new("m");
        let sse_text = feed(
            &mut t,
            &[
                r#"{"choices":[{"delta":{"content":"我来读"}}]}"#,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"Read","arguments":"{}"}}]}}]}"#,
            ],
        );
        let names = event_names(&sse_text);
        let stop_at = names.iter().position(|n| n == "content_block_stop").expect("文本块要收掉");
        let start_at = names.iter().rposition(|n| n == "content_block_start").expect("工具块要开");
        assert!(stop_at < start_at, "工具块开之前必须先关文本块：{names:?}");
    }

    #[test]
    fn parallel_tool_calls_get_distinct_block_indices() {
        let mut t = StreamTranslator::new("m");
        let mut sse_text = feed(
            &mut t,
            &[
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"Read","arguments":"{}"}},{"index":1,"id":"c2","function":{"name":"Write","arguments":"{}"}}]}}]}"#,
                r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            ],
        );
        sse_text.push_str(&t.finish());
        let evts = events(&sse_text);
        let indices: Vec<i64> = evts
            .iter()
            .filter(|(n, _)| n == "content_block_start")
            .map(|(_, d)| d["index"].as_i64().unwrap())
            .collect();
        assert_eq!(indices, vec![0, 1], "并行工具调用必须各占一个块索引");
        let stops: Vec<i64> = evts
            .iter()
            .filter(|(n, _)| n == "content_block_stop")
            .map(|(_, d)| d["index"].as_i64().unwrap())
            .collect();
        assert_eq!(stops, vec![0, 1], "开了的块都要收掉");
        let delta = evts.iter().find(|(n, _)| n == "message_delta").expect("要有 message_delta");
        assert_eq!(delta.1["delta"]["stop_reason"], "tool_use");
    }

    #[test]
    fn arguments_arriving_before_the_name_are_buffered_not_lost() {
        // 少见但合法：先来一片没有 name 的参数。发一个没名字的 tool_use
        // 客户端没法用，所以先攒着，拿到 name 再一起发。
        let mut t = StreamTranslator::new("m");
        let sse_text = feed(
            &mut t,
            &[
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"a\":"}}]}}]}"#,
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"F","arguments":"1}"}}]}}]}"#,
            ],
        );
        let evts = events(&sse_text);
        assert_eq!(
            evts.iter().find(|(n, _)| n == "content_block_start").unwrap().1["content_block"]["name"],
            "F"
        );
        let joined: String = evts
            .iter()
            .filter(|(n, d)| n == "content_block_delta" && d["delta"]["type"] == "input_json_delta")
            .map(|(_, d)| d["delta"]["partial_json"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(serde_json::from_str::<Value>(&joined).unwrap()["a"], 1, "早到的分片不能丢");
    }

    #[test]
    fn chunk_split_across_tcp_boundaries_is_reassembled() {
        let mut t = StreamTranslator::new("m");
        let mut out = t.push_bytes(b"data: {\"choices\":[{\"delta\":{\"cont");
        assert!(events(&out).iter().all(|(n, _)| n != "content_block_delta"));
        out.push_str(&t.push_bytes("ent\":\"半行\"}}]}\n".as_bytes()));
        let text: Vec<String> = events(&out)
            .iter()
            .filter(|(n, _)| n == "content_block_delta")
            .map(|(_, d)| d["delta"]["text"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(text, vec!["半行"]);
    }

    #[test]
    fn finish_is_idempotent() {
        // [DONE] 和流自然结束都会触发收尾，发两遍客户端会看到两个 message_stop。
        let mut t = StreamTranslator::new("m");
        let mut out = t.push_bytes(b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n");
        out.push_str(&t.push_bytes(b"data: [DONE]\n"));
        out.push_str(&t.finish());
        let stops = events(&out).iter().filter(|(n, _)| n == "message_stop").count();
        assert_eq!(stops, 1);
    }

    // ── 接缝 ──

    /// 纯函数各自对了，不代表**接线**对。handler 里那一步
    /// （`AnthropicRequest` → `serde_json::to_value(&m.content)` → 本模块）
    /// 是最可能出错的地方：`AnthropicMessageContent` 是 untagged 枚举、块里还有
    /// `#[serde(flatten)] extra`，序列化回来的形状必须正好是这里认得的样子。
    ///
    /// 本机没有 OpenAI 兼容上游可以打，这条就是能验到的最靠外的一环。
    #[test]
    fn a_real_claude_code_request_survives_the_whole_seam() {
        use crate::proxy_types::AnthropicRequest;

        // Claude Code 一轮带工具往返的真实形状。
        let raw = json!({
            "model": "claude-x",
            "max_tokens": 1024,
            "system": "你是编码助手",
            "tools": [{
                "name": "Read",
                "description": "读文件",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }],
            "tool_choice": {"type": "auto"},
            "messages": [
                {"role": "user", "content": "看一下 a.txt"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "tu_1", "name": "Read", "input": {"path": "a.txt"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1", "content": "文件内容"}
                ]}
            ]
        });

        let parsed: AnthropicRequest = serde_json::from_value(raw).expect("网关能解析");
        // 下面三行与 proxy.rs 里 handler 的做法逐字一致。
        let system = parsed.system.as_ref().map(|s| Value::String(s.to_string_content()));
        let messages: Vec<(String, Value)> = parsed
            .messages
            .iter()
            .map(|m| (m.role.clone(), serde_json::to_value(&m.content).unwrap()))
            .collect();
        let out = messages_to_openai(system.as_ref(), &messages);

        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[1]["content"], "看一下 a.txt");
        // tool_use 必须活着穿过 untagged 枚举 + flatten extra 这一层。
        assert_eq!(out[2]["tool_calls"][0]["function"]["name"], "Read");
        let args = out[2]["tool_calls"][0]["function"]["arguments"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(args).unwrap()["path"], "a.txt");
        assert_eq!(out[3]["role"], "tool");
        assert_eq!(out[3]["tool_call_id"], "tu_1");

        // 工具定义与 tool_choice 也走同一条接线。
        let tools = tools_to_openai(parsed.extra.get("tools").unwrap()).unwrap();
        assert_eq!(tools[0]["function"]["name"], "Read");
        assert_eq!(
            tool_choice_to_openai(parsed.extra.get("tool_choice").unwrap()).unwrap(),
            json!("auto")
        );
    }

    #[test]
    fn absurd_tool_index_from_upstream_cannot_blow_up_memory() {
        let mut t = StreamTranslator::new("m");
        let out = feed(
            &mut t,
            &[r#"{"choices":[{"delta":{"tool_calls":[{"index":99999999,"id":"c","function":{"name":"F"}}]}}]}"#],
        );
        // 只应有 message_start，没有为这个荒唐索引分配任何东西。
        assert!(events(&out).iter().all(|(n, _)| n == "message_start"));
    }
}
