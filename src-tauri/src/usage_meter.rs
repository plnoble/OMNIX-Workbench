//! 网关用量计量：把上游真实返回的 token 数抠出来。
//!
//! ## 为什么要有这个模块
//!
//! `request_logs` 表一直有 token 列、`log_request` 一直收 token 参数，
//! 但网关里三处调用全都硬编码传 `0, 0`——**表在、字段在、读取端在，
//! 唯独没有一个写入端传过真值**。仪表盘上的用量和成本因此恒为零。
//!
//! 就地 parse 修不了这个，因为网关有四条出口形状，usage 的位置各不相同：
//!
//! | 出口 | usage 在哪 |
//! |---|---|
//! | Anthropic 直通 · 非流式 | 响应体 `.usage` |
//! | Anthropic 直通 · 流式   | `message_start` 里给输入，`message_delta` 里给累计输出 |
//! | OpenAI 直通 · 非流式    | 响应体 `.usage`（字段名不同） |
//! | OpenAI 直通 · 流式      | 末尾 chunk 的 `.usage`（上游未开 `include_usage` 时压根没有） |
//!
//! 四处各写一遍，必然写歪三遍。所以抽成纯函数 + 一个流式扫描器。
//!
//! ## 关于「输入 token」的口径
//!
//! Anthropic 把输入拆成三份：`input_tokens`、`cache_read_input_tokens`、
//! `cache_creation_input_tokens`。Claude Code 这类长上下文 agent 绝大部分输入
//! 走缓存命中——只记 `input_tokens` 会把一次真实几万 token 的请求记成个位数，
//! 那比记零更糟：它看起来是对的。
//!
//! 所以 [`UsageTally::billable_input`] 返回三者之和，写进 `prompt_tokens`；
//! 明细另存两列。这样所有既有读取端（`total_tokens`、`estimate_cost`、仪表盘）
//! 不用改就是对的。

use serde_json::Value;

/// 一次请求的 token 用量。字段都是**累计总量**，不是增量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageTally {
    /// 未命中缓存的新输入。
    pub input: i64,
    /// 输出。
    pub output: i64,
    /// 缓存命中读取（Anthropic 计费约为普通输入的 10%）。
    pub cache_read: i64,
    /// 写入缓存（Anthropic 计费约为普通输入的 125%）。
    pub cache_creation: i64,
}

impl UsageTally {
    /// 计费口径的输入总量——见模块文档里「输入 token 的口径」。
    pub fn billable_input(&self) -> i64 {
        self.input + self.cache_read + self.cache_creation
    }

    /// 是否拿到过任何真实数字。上游没给 usage 时不要把零写进库，
    /// 否则「没拿到」和「真的是零」在表里长得一模一样。
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// 按字段取较大值合并。
    ///
    /// 流式响应里 usage 是**逐步累计**的（`message_delta.usage.output_tokens`
    /// 每次都是到目前为止的总数），取 max 既能吸收累计更新，又不会被末尾某个
    /// 缺字段的事件清零。
    fn absorb(&mut self, other: UsageTally) {
        self.input = self.input.max(other.input);
        self.output = self.output.max(other.output);
        self.cache_read = self.cache_read.max(other.cache_read);
        self.cache_creation = self.cache_creation.max(other.cache_creation);
    }
}

fn num(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|n| n.as_i64()).unwrap_or(0)
}

/// 从 Anthropic 形状的 `usage` 对象读取。
fn anthropic_usage_obj(usage: &Value) -> UsageTally {
    UsageTally {
        input: num(usage, "input_tokens"),
        output: num(usage, "output_tokens"),
        cache_read: num(usage, "cache_read_input_tokens"),
        cache_creation: num(usage, "cache_creation_input_tokens"),
    }
}

/// 从 OpenAI 形状的 `usage` 对象读取。
///
/// OpenAI 的 `prompt_tokens` **已经含**缓存命中部分，缓存数在
/// `prompt_tokens_details.cached_tokens` 里单列。所以这里要减出来，
/// 让 [`UsageTally::input`] 的语义（未命中的新输入）与 Anthropic 一致，
/// 相加后仍等于上游的 `prompt_tokens`。
fn openai_usage_obj(usage: &Value) -> UsageTally {
    let prompt = num(usage, "prompt_tokens");
    let cached = usage
        .get("prompt_tokens_details")
        .map(|d| num(d, "cached_tokens"))
        .unwrap_or(0)
        .clamp(0, prompt);
    UsageTally {
        input: prompt - cached,
        output: num(usage, "completion_tokens"),
        cache_read: cached,
        cache_creation: 0,
    }
}

/// 从一整个非流式响应体里取 usage。两种上游形状都试，谁认得算谁的。
///
/// 之所以不按 `api_type` 分派：中转站常把 OpenAI 后端套上 Anthropic 外壳
/// （反之亦然），按配置分派会在这种混搭上取空。按**响应实际长什么样**判断更准。
pub fn from_response_body(bytes: &[u8]) -> Option<UsageTally> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    let usage = root.get("usage")?;
    let tally = merge_usage_shapes(usage);
    (!tally.is_empty()).then_some(tally)
}

/// 同一个 `usage` 对象上把两种形状都读一遍再合并。
/// 字段名不重叠，所以不会互相污染；混搭外壳的中转站也能取到。
fn merge_usage_shapes(usage: &Value) -> UsageTally {
    let mut tally = anthropic_usage_obj(usage);
    tally.absorb(openai_usage_obj(usage));
    tally
}

/// 单行 SSE `data:` 载荷里可能藏着的 usage。
fn from_sse_payload(payload: &str) -> Option<UsageTally> {
    let root: Value = serde_json::from_str(payload).ok()?;
    // Anthropic `message_start` 把 usage 埋在 `.message.usage`；
    // `message_delta` 和 OpenAI 末尾 chunk 都在顶层 `.usage`。
    let usage = root
        .get("message")
        .and_then(|m| m.get("usage"))
        .or_else(|| root.get("usage"))?;
    let tally = merge_usage_shapes(usage);
    (!tally.is_empty()).then_some(tally)
}

/// 单行 SSE 的上限。正常 `data:` 行远小于此；超过说明上游要么在发
/// 非 SSE 内容，要么坏了——不能让它把网关的内存吃光。
const MAX_SSE_LINE: usize = 1024 * 1024;

/// 流式响应的旁路扫描器：字节照原样往下游走，顺手把 usage 记下来。
///
/// 按行缓冲，因为一个 SSE 事件可以被 TCP 切在任意字节上。
#[derive(Debug, Default)]
pub struct SseUsageScanner {
    buffer: Vec<u8>,
    tally: UsageTally,
    /// 缓冲曾经溢出过——溢出时丢弃的那段里可能有 usage，读数不再可信。
    overflowed: bool,
}

impl SseUsageScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂一段原始字节。**只读不改**——下游收到的仍是上游原样的字节。
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&self.buffer[..pos]).trim().to_string();
            self.buffer.drain(..pos + 1);
            if let Some(payload) = line.strip_prefix("data:") {
                let payload = payload.trim();
                if payload != "[DONE]" {
                    if let Some(found) = from_sse_payload(payload) {
                        self.tally.absorb(found);
                    }
                }
            }
        }
        if self.buffer.len() > MAX_SSE_LINE {
            self.buffer.clear();
            self.overflowed = true;
        }
    }

    /// 目前累计到的用量。溢出过就返回 `None`——宁可记「没拿到」，
    /// 也不要把一个自己都知道是残的数字写进用量统计。
    pub fn tally(&self) -> Option<UsageTally> {
        if self.overflowed || self.tally.is_empty() {
            None
        } else {
            Some(self.tally)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_non_stream_body_yields_all_four_numbers() {
        let body = br#"{"type":"message","content":[],"usage":{
            "input_tokens": 12,
            "output_tokens": 340,
            "cache_read_input_tokens": 45000,
            "cache_creation_input_tokens": 1200
        }}"#;
        let t = from_response_body(body).expect("应当解析出 usage");
        assert_eq!(t.input, 12);
        assert_eq!(t.output, 340);
        assert_eq!(t.cache_read, 45000);
        assert_eq!(t.cache_creation, 1200);
        // 这正是「只记 input_tokens 会记成个位数」的那种请求。
        assert_eq!(t.billable_input(), 46212);
    }

    #[test]
    fn openai_cached_tokens_are_split_out_not_double_counted() {
        let body = br#"{"usage":{
            "prompt_tokens": 1000,
            "completion_tokens": 50,
            "prompt_tokens_details": {"cached_tokens": 800}
        }}"#;
        let t = from_response_body(body).expect("应当解析出 usage");
        assert_eq!(t.input, 200, "未命中部分 = prompt - cached");
        assert_eq!(t.cache_read, 800);
        // 关键不变量：拆开再合起来，等于上游给的 prompt_tokens。
        assert_eq!(t.billable_input(), 1000);
    }

    #[test]
    fn body_without_usage_reports_nothing_rather_than_zero() {
        assert!(from_response_body(br#"{"content":[{"type":"text","text":"hi"}]}"#).is_none());
        assert!(from_response_body(b"not json at all").is_none());
        // 上游给了 usage 但全是零，同样按「没拿到」处理：写零进库会让
        // 「上游没报」和「真的没用 token」在表里无法区分。
        assert!(from_response_body(br#"{"usage":{"input_tokens":0,"output_tokens":0}}"#).is_none());
    }

    #[test]
    fn anthropic_sse_combines_message_start_and_message_delta() {
        let mut s = SseUsageScanner::new();
        s.feed(b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7,\"cache_read_input_tokens\":9000}}}\n\n");
        s.feed(b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"x\"}}\n\n");
        s.feed(b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":128}}\n\n");
        let t = s.tally().expect("流式也应当拿到用量");
        assert_eq!(t.input, 7);
        assert_eq!(t.cache_read, 9000);
        assert_eq!(t.output, 128, "输入来自 message_start，输出来自 message_delta");
    }

    #[test]
    fn sse_event_split_across_chunks_is_still_read() {
        // TCP 可以把一个事件切在任意字节上，这里故意切在 JSON 中间。
        let mut s = SseUsageScanner::new();
        s.feed(b"data: {\"usage\":{\"input_to");
        assert!(s.tally().is_none(), "半行不该产出读数");
        s.feed(b"kens\":42,\"output_tokens\":8}}\n");
        let t = s.tally().expect("行补全后应当解析出来");
        assert_eq!(t.input, 42);
        assert_eq!(t.output, 8);
    }

    #[test]
    fn cumulative_output_takes_the_largest_not_the_sum() {
        // Anthropic 的 output_tokens 是累计值。累加会把 10+20+30 记成 60。
        let mut s = SseUsageScanner::new();
        for n in [10, 20, 30] {
            s.feed(format!("data: {{\"usage\":{{\"output_tokens\":{n}}}}}\n").as_bytes());
        }
        assert_eq!(s.tally().expect("应有读数").output, 30);
    }

    #[test]
    fn stream_without_usage_reports_nothing() {
        let mut s = SseUsageScanner::new();
        s.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n");
        assert!(s.tally().is_none(), "上游没开 include_usage 时应当如实报「没拿到」");
    }

    #[test]
    fn runaway_line_is_dropped_and_poisons_the_reading() {
        let mut s = SseUsageScanner::new();
        s.feed(b"data: {\"usage\":{\"input_tokens\":5}}\n");
        assert!(s.tally().is_some());
        // 上游开始发不带换行的垃圾：内存要保得住，且读数要作废而不是留个残值。
        s.feed(&vec![b'x'; MAX_SSE_LINE + 1]);
        assert!(s.tally().is_none(), "丢过数据之后不能再报一个看似正常的数字");
    }

    #[test]
    fn openai_stream_final_chunk_usage_is_picked_up() {
        let mut s = SseUsageScanner::new();
        s.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n");
        s.feed(b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":300,\"completion_tokens\":20}}\n");
        s.feed(b"data: [DONE]\n");
        let t = s.tally().expect("末尾 chunk 带 usage 时应当拿到");
        assert_eq!(t.billable_input(), 300);
        assert_eq!(t.output, 20);
    }
}
