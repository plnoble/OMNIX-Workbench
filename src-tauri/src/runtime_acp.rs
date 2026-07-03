//! Universal ACP (Agent Client Protocol) adapter.
//!
//! ACP is a JSON-RPC 2.0 protocol spoken over stdio. A single implementation of
//! this adapter drives every ACP-native agent (Gemini CLI, Qwen Code, OpenCode,
//! GitHub Copilot CLI, and any future ACP agent), because the wire contract is
//! shared.
//!
//! Unlike the Claude / Codex adapters this transport is **bidirectional**: after
//! the client establishes a session, the agent issues requests back to the
//! client (`fs/read_text_file`, `fs/write_text_file`, `session/request_permission`)
//! that the client must answer. We drive the wire ourselves over the same
//! stdin/stdout line-JSON loop used by the Codex adapter (rather than the full
//! `agent-client-protocol` SDK, whose connection driver owns its own event loop
//! and does not fit our spawn-and-dispatch runtime model).
//!
//! Outgoing requests are constructed with `serde_json::json!` (matching the
//! Codex builders and avoiding the schema's `#[non_exhaustive]` builder
//! friction). The polymorphic `session/update` notification stream — where a
//! typo in a discriminant would silently drop updates — is parsed through the
//! typed `agent-client-protocol-schema` messages for safety, with a raw-log
//! fallback so protocol drift never breaks the reader.

use std::path::{Component, Path, PathBuf};

use agent_client_protocol_schema::v1::{
    ContentBlock, Plan, SessionNotification, SessionUpdate, ToolCallStatus,
};
use serde_json::{json, Value};

use crate::runtime::{RuntimeEvent, RuntimeEventKind};

/// JSON-RPC method the agent uses to ask the client to read a file.
const FS_READ_TEXT_FILE: &str = "fs/read_text_file";
/// JSON-RPC method the agent uses to ask the client to write a file.
const FS_WRITE_TEXT_FILE: &str = "fs/write_text_file";
/// JSON-RPC method the agent uses to request a permission decision.
const SESSION_REQUEST_PERMISSION: &str = "session/request_permission";
/// JSON-RPC streaming notification carrying assistant/tool/plan updates.
const SESSION_UPDATE: &str = "session/update";

/// JSON-RPC "method not found" error code, per the JSON-RPC 2.0 spec.
pub const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC "internal error" error code, per the JSON-RPC 2.0 spec.
pub const JSONRPC_INTERNAL_ERROR: i64 = -32603;

/// Classification of a single inbound line from an ACP agent.
///
/// The reader loop consumes this: [`AcpInbound::Emit`] events are persisted and
/// published like any other runtime event, while the request variants require
/// the client to write a JSON-RPC response back over stdin.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpInbound {
    /// Runtime events distilled from a notification or a response. The reader
    /// persists + publishes these; any event carrying an `external_session_id`
    /// also updates the session's stored ACP session id.
    Emit(Vec<RuntimeEvent>),
    /// The agent asked the client to read a file; the client must reply with the
    /// file contents (or an error if the path escapes the workspace).
    ReadFile {
        request_id: Value,
        path: String,
        line: Option<u32>,
        limit: Option<u32>,
    },
    /// The agent asked the client to write a file; the client must reply once the
    /// write completes (or with an error).
    WriteFile {
        request_id: Value,
        path: String,
        content: String,
    },
    /// The agent asked for a permission decision. The manager applies the
    /// session's permission policy: auto-approve under confirmed full access,
    /// auto-reject in plan mode, otherwise surface `event` for the user and
    /// answer later via `respond_approval`. `options` drives option selection.
    Permission {
        request_id: Value,
        options: Value,
        event: RuntimeEvent,
    },
    /// An agent→client request the adapter does not implement. The client must
    /// still answer with a JSON-RPC error so the agent is not left hanging.
    UnsupportedRequest { request_id: Value, method: String },
}

/// Classifies one line of ACP JSON-RPC traffic coming from the agent.
///
/// Never errors on well-formed JSON: anything unrecognized becomes a raw-log
/// [`AcpInbound::Emit`] so the reader keeps a faithful audit trail.
pub fn classify_acp_message(line: &str) -> Result<AcpInbound, String> {
    let value: Value =
        serde_json::from_str(line).map_err(|error| format!("invalid ACP JSON: {error}"))?;
    let method = value.get("method").and_then(Value::as_str);
    let has_request_id = value.get("id").is_some_and(|id| !id.is_null());

    match (method, has_request_id) {
        // Agent → client request (must be answered).
        (Some(method), true) => {
            let request_id = value.get("id").cloned().unwrap_or(Value::Null);
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            Ok(classify_agent_request(method, request_id, &params))
        }
        // Streaming update notification.
        (Some(SESSION_UPDATE), false) => Ok(AcpInbound::Emit(parse_session_update(&value))),
        // Any other notification: keep it as a raw log for auditing.
        (Some(_), false) => Ok(AcpInbound::Emit(vec![raw_log(line, &value)])),
        // Response to one of our outgoing requests (has id + result/error).
        (None, _) => Ok(AcpInbound::Emit(parse_response(&value, line))),
    }
}

fn classify_agent_request(method: &str, request_id: Value, params: &Value) -> AcpInbound {
    match method {
        FS_READ_TEXT_FILE => AcpInbound::ReadFile {
            request_id,
            path: string_field(params, "path"),
            line: u32_field(params, "line"),
            limit: u32_field(params, "limit"),
        },
        FS_WRITE_TEXT_FILE => AcpInbound::WriteFile {
            request_id,
            path: string_field(params, "path"),
            content: string_field(params, "content"),
        },
        SESSION_REQUEST_PERMISSION => {
            let options = params.get("options").cloned().unwrap_or(Value::Null);
            let mut event = RuntimeEvent::new(
                RuntimeEventKind::ApprovalRequested,
                json!({
                    "adapter": "acp",
                    "approval_method": SESSION_REQUEST_PERMISSION,
                    "options": options.clone(),
                    "tool_call": params.get("toolCall").cloned().unwrap_or(Value::Null),
                }),
            );
            event.request_id = Some(json_id_to_string(&request_id));
            event.external_session_id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
            event.text = params
                .pointer("/toolCall/title")
                .or_else(|| params.pointer("/toolCall/fields/title"))
                .and_then(Value::as_str)
                .map(str::to_string);
            AcpInbound::Permission {
                request_id,
                options,
                event,
            }
        }
        other => AcpInbound::UnsupportedRequest {
            request_id,
            method: other.to_string(),
        },
    }
}

/// Parses a `session/update` notification into runtime events, using the typed
/// schema for the polymorphic update payload and falling back to a raw log if
/// the agent sends a shape this version of the schema does not recognize.
fn parse_session_update(envelope: &Value) -> Vec<RuntimeEvent> {
    let params = envelope
        .get("params")
        .cloned()
        .unwrap_or(Value::Null);
    let notification: SessionNotification = match serde_json::from_value(params) {
        Ok(notification) => notification,
        Err(error) => {
            let mut event = raw_log(&envelope.to_string(), envelope);
            event.metadata = json!({ "acp_parse_error": error.to_string(), "raw": envelope });
            return vec![event];
        }
    };
    let session_id = notification.session_id.to_string();

    let mut event = match notification.update {
        // The three chunk variants are the streaming hot path: hundreds of events
        // per reply, each persisted as its own runtime_events row. Their metadata
        // deliberately carries only the chunk tag — cloning the full envelope here
        // multiplied the database size by the envelope/delta ratio (~10-40x).
        SessionUpdate::AgentMessageChunk(chunk) => {
            // Tagged "message" so the manager accumulates it into the consolidated
            // AssistantMessage that gets persisted to `messages` at turn end.
            let mut event = RuntimeEvent::new(
                RuntimeEventKind::AssistantDelta,
                json!({ "acp_chunk": "message" }),
            );
            event.text = content_block_text(&chunk.content);
            event.item_id = chunk.message_id.map(|id| id.to_string());
            event
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            // Reasoning: streamed live but deliberately not persisted as message text.
            let mut event = RuntimeEvent::new(
                RuntimeEventKind::AssistantDelta,
                json!({ "acp_chunk": "thought" }),
            );
            event.text = content_block_text(&chunk.content);
            event
        }
        SessionUpdate::UserMessageChunk(chunk) => {
            let mut event =
                RuntimeEvent::new(RuntimeEventKind::UserMessage, json!({ "acp_chunk": "user" }));
            event.text = content_block_text(&chunk.content);
            event
        }
        SessionUpdate::ToolCall(call) => {
            let mut event = RuntimeEvent::new(RuntimeEventKind::ToolStarted, envelope.clone());
            event.item_id = Some(call.tool_call_id.to_string());
            event.text = Some(call.title);
            event
        }
        SessionUpdate::ToolCallUpdate(update) => {
            let kind = match update.fields.status {
                Some(ToolCallStatus::Completed) | Some(ToolCallStatus::Failed) => {
                    RuntimeEventKind::ToolCompleted
                }
                _ => RuntimeEventKind::ToolStarted,
            };
            let mut event = RuntimeEvent::new(kind, envelope.clone());
            event.item_id = Some(update.tool_call_id.to_string());
            event.text = update.fields.title;
            event
        }
        SessionUpdate::Plan(plan) => {
            let mut event = RuntimeEvent::new(RuntimeEventKind::Plan, envelope.clone());
            event.text = Some(summarize_plan(&plan));
            event
        }
        // Session mode / config / usage / commands updates and any future
        // (`#[non_exhaustive]`) variants are recorded verbatim for audit.
        _ => {
            let mut event = RuntimeEvent::new(RuntimeEventKind::RawLog, envelope.clone());
            event.text = None;
            event
        }
    };
    event.external_session_id = Some(session_id);
    vec![event]
}

/// Parses a JSON-RPC response to one of our outgoing requests. `session/new` and
/// `session/load` carry the ACP session id; `session/prompt` carries the turn's
/// stop reason; errors surface as an [`RuntimeEventKind::Error`] event.
fn parse_response(value: &Value, line: &str) -> Vec<RuntimeEvent> {
    if let Some(session_id) = value.pointer("/result/sessionId").and_then(Value::as_str) {
        let mut event = RuntimeEvent::new(RuntimeEventKind::SessionStarted, value.clone());
        event.external_session_id = Some(session_id.to_string());
        // Capture the agent's selectable model, if it exposes one via the
        // standard `configOptions`, so OMNIX can present a model picker and set
        // it with `session/set_config_option` (e.g. opencode; agents without
        // configOptions like Gemini simply run on their own default).
        if let Some(model) = extract_model_config_option(value) {
            event.metadata = json!({ "acp_model_option": model, "raw": value });
        }
        return vec![event];
    }
    if let Some(stop_reason) = value.pointer("/result/stopReason").and_then(Value::as_str) {
        let mut event = RuntimeEvent::new(RuntimeEventKind::TurnCompleted, value.clone());
        event.text = Some(stop_reason.to_string());
        return vec![event];
    }
    if let Some(error) = value.get("error") {
        let mut event = RuntimeEvent::new(RuntimeEventKind::Error, value.clone());
        event.text = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string);
        return vec![event];
    }
    // `initialize` response and any other successful result: keep as raw log.
    let mut event = RuntimeEvent::new(RuntimeEventKind::RawLog, value.clone());
    event.text = Some(line.to_string());
    vec![event]
}

/// Extracts the model selection config option from a `session/new` response,
/// returning `{configId, current, options:[{value,name}]}` when the agent
/// exposes a `configOptions` entry of category/id `model`.
fn extract_model_config_option(value: &Value) -> Option<Value> {
    let options = value.pointer("/result/configOptions")?.as_array()?;
    let model = options.iter().find(|option| {
        option.get("category").and_then(Value::as_str) == Some("model")
            || option.get("id").and_then(Value::as_str) == Some("model")
    })?;
    let choices: Vec<Value> = model
        .get("options")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|choice| {
                    let choice_value = choice.get("value").and_then(Value::as_str)?;
                    let name = choice
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(choice_value);
                    Some(json!({ "value": choice_value, "name": name }))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(json!({
        "config_id": model.get("id").and_then(Value::as_str).unwrap_or("model"),
        "current": model.get("currentValue").and_then(Value::as_str),
        "options": choices,
    }))
}

fn content_block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

fn summarize_plan(plan: &Plan) -> String {
    plan.entries
        .iter()
        .map(|entry| format!("- {}", entry.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn raw_log(line: &str, value: &Value) -> RuntimeEvent {
    let mut event = RuntimeEvent::new(RuntimeEventKind::RawLog, value.clone());
    event.text = Some(line.to_string());
    event
}

fn string_field(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn u32_field(params: &Value, key: &str) -> Option<u32> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
}

/// Renders a JSON-RPC id as the stored string form — shared with the Codex
/// adapter (see `crate::runtime::json_id_to_string`). Numeric ids round-trip
/// losslessly because [`request_id_from_string`] parses them back.
pub(crate) use crate::runtime::json_id_to_string;

/// Converts a stored approval id string back into the JSON-RPC id the agent
/// expects (a number when it parses as one, otherwise the original string).
pub fn request_id_from_string(request_id: &str) -> Value {
    request_id
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::String(request_id.to_string()))
}

// ---------------------------------------------------------------------------
// Outgoing message construction
// ---------------------------------------------------------------------------

pub fn build_acp_initialize_request(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": { "readTextFile": true, "writeTextFile": true },
                "terminal": false
            },
            "clientInfo": {
                "name": "omnix-workbench",
                "title": "OMNIX Workbench",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

pub fn build_acp_new_session_request(id: u64, cwd: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/new",
        "params": { "cwd": cwd, "mcpServers": [] }
    })
}

pub fn build_acp_prompt_request(id: u64, session_id: &str, prompt: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [ { "type": "text", "text": prompt } ]
        }
    })
}

/// Selects a session config option (e.g. the model) on a live ACP session.
pub fn build_acp_set_config_option_request(
    id: u64,
    session_id: &str,
    config_id: &str,
    value: &str,
) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/set_config_option",
        "params": { "sessionId": session_id, "configId": config_id, "value": value }
    })
}

pub fn build_acp_cancel_notification(session_id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/cancel",
        "params": { "sessionId": session_id }
    })
}

pub fn build_acp_read_file_response(request_id: &Value, content: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": request_id, "result": { "content": content } })
}

pub fn build_acp_write_file_response(request_id: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": request_id, "result": {} })
}

/// Builds the response to `session/request_permission`. `Some(option_id)` selects
/// an offered option; `None` reports the turn was cancelled before a decision.
pub fn build_acp_permission_response(request_id: &Value, option_id: Option<&str>) -> Value {
    let outcome = match option_id {
        Some(option_id) => json!({ "outcome": "selected", "optionId": option_id }),
        None => json!({ "outcome": "cancelled" }),
    };
    json!({ "jsonrpc": "2.0", "id": request_id, "result": { "outcome": outcome } })
}

pub fn build_acp_error_response(request_id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": request_id, "error": { "code": code, "message": message } })
}

/// Chooses the option id to send back for a boolean approve/reject decision,
/// preferring the "always" variant when the user opted to remember the choice.
/// Returns `None` if the agent offered no option matching the decision.
pub fn select_permission_option(options: &Value, approved: bool, for_session: bool) -> Option<String> {
    let list = options.as_array()?;
    let find = |wanted: &[&str]| -> Option<String> {
        for kind in wanted {
            for option in list {
                if option.get("kind").and_then(Value::as_str) == Some(*kind) {
                    if let Some(id) = option.get("optionId").and_then(Value::as_str) {
                        return Some(id.to_string());
                    }
                }
            }
        }
        None
    };
    match (approved, for_session) {
        (true, true) => find(&["allow_always", "allow_once"]),
        (true, false) => find(&["allow_once", "allow_always"]),
        (false, true) => find(&["reject_always", "reject_once"]),
        (false, false) => find(&["reject_once", "reject_always"]),
    }
}

// ---------------------------------------------------------------------------
// Workspace path safety
// ---------------------------------------------------------------------------

/// Resolves an agent-supplied path against the session workspace and rejects any
/// path that escapes it. Uses lexical normalization (not `canonicalize`) so that
/// `fs/write_text_file` can create files that do not yet exist.
pub fn resolve_workspace_path(workspace_root: &str, requested: &str) -> Result<PathBuf, String> {
    if requested.trim().is_empty() {
        return Err("路径不能为空".into());
    }
    let root = normalize(Path::new(workspace_root));
    let requested_path = Path::new(requested);
    let joined = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root.join(requested_path)
    };
    let normalized = normalize(&joined);
    if !normalized.starts_with(&root) {
        return Err(format!("路径越界，超出工作区范围: {requested}"));
    }
    Ok(normalized)
}

/// Lexically normalizes a path by resolving `.` and `..` components without
/// touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeEventKind;

    #[test]
    fn prompt_request_carries_text_content_block() {
        let request = build_acp_prompt_request(7, "sess-1", "hello");
        assert_eq!(request["method"], "session/prompt");
        assert_eq!(request["params"]["sessionId"], "sess-1");
        assert_eq!(request["params"]["prompt"][0]["type"], "text");
        assert_eq!(request["params"]["prompt"][0]["text"], "hello");
    }

    #[test]
    fn initialize_advertises_fs_capability() {
        let request = build_acp_initialize_request(1);
        assert_eq!(request["params"]["protocolVersion"], 1);
        assert_eq!(request["params"]["clientCapabilities"]["fs"]["readTextFile"], true);
        assert_eq!(request["params"]["clientCapabilities"]["fs"]["writeTextFile"], true);
    }

    #[test]
    fn classify_read_file_request() {
        let line = r#"{"jsonrpc":"2.0","id":9,"method":"fs/read_text_file","params":{"sessionId":"s","path":"src/main.rs","line":2,"limit":10}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::ReadFile { request_id, path, line, limit } => {
                assert_eq!(request_id, Value::from(9));
                assert_eq!(path, "src/main.rs");
                assert_eq!(line, Some(2));
                assert_eq!(limit, Some(10));
            }
            other => panic!("expected ReadFile, got {other:?}"),
        }
    }

    #[test]
    fn classify_write_file_request() {
        let line = r#"{"jsonrpc":"2.0","id":"w1","method":"fs/write_text_file","params":{"sessionId":"s","path":"out.txt","content":"data"}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::WriteFile { request_id, path, content } => {
                assert_eq!(request_id, Value::from("w1"));
                assert_eq!(path, "out.txt");
                assert_eq!(content, "data");
            }
            other => panic!("expected WriteFile, got {other:?}"),
        }
    }

    #[test]
    fn classify_permission_request_records_options_and_id() {
        let line = r#"{"jsonrpc":"2.0","id":42,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"tc1","title":"Edit main.rs"},"options":[{"optionId":"o-allow","name":"Allow","kind":"allow_once"},{"optionId":"o-reject","name":"Reject","kind":"reject_once"}]}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Permission { request_id, options, event } => {
                assert_eq!(request_id, Value::from(42));
                assert_eq!(options[0]["optionId"], "o-allow");
                assert_eq!(event.kind, RuntimeEventKind::ApprovalRequested);
                assert_eq!(event.request_id.as_deref(), Some("42"));
                assert_eq!(event.text.as_deref(), Some("Edit main.rs"));
                assert_eq!(event.metadata["approval_method"], SESSION_REQUEST_PERMISSION);
            }
            other => panic!("expected Permission, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_agent_request_is_flagged() {
        let line = r#"{"jsonrpc":"2.0","id":5,"method":"terminal/create","params":{}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::UnsupportedRequest { request_id, method } => {
                assert_eq!(request_id, Value::from(5));
                assert_eq!(method, "terminal/create");
            }
            other => panic!("expected UnsupportedRequest, got {other:?}"),
        }
    }

    #[test]
    fn session_update_agent_message_chunk_maps_to_delta() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-9","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"partial"},"messageId":"m1"}}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                let event = &events[0];
                assert_eq!(event.kind, RuntimeEventKind::AssistantDelta);
                assert_eq!(event.text.as_deref(), Some("partial"));
                assert_eq!(event.item_id.as_deref(), Some("m1"));
                assert_eq!(event.external_session_id.as_deref(), Some("sess-9"));
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn session_update_tool_call_maps_to_tool_started() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"call-1","title":"Run tests","kind":"execute","status":"pending"}}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                let event = &events[0];
                assert_eq!(event.kind, RuntimeEventKind::ToolStarted);
                assert_eq!(event.item_id.as_deref(), Some("call-1"));
                assert_eq!(event.text.as_deref(), Some("Run tests"));
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn session_update_tool_call_update_completed_maps_to_tool_completed() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"call-1","status":"completed","title":"Ran tests"}}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                let event = &events[0];
                assert_eq!(event.kind, RuntimeEventKind::ToolCompleted);
                assert_eq!(event.item_id.as_deref(), Some("call-1"));
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn unknown_update_variant_falls_back_to_raw_log() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"some_future_kind","payload":{}}}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                assert_eq!(events[0].kind, RuntimeEventKind::RawLog);
                assert!(events[0].metadata.get("acp_parse_error").is_some());
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn new_session_response_yields_session_id() {
        let line = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"acp-session-123"}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                let event = &events[0];
                assert_eq!(event.kind, RuntimeEventKind::SessionStarted);
                assert_eq!(event.external_session_id.as_deref(), Some("acp-session-123"));
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn session_new_captures_model_config_option() {
        let line = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_x","configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"opencode/big-pickle","options":[{"value":"opencode/big-pickle","name":"Big Pickle"},{"value":"opencode/deepseek-v4-flash-free","name":"DeepSeek Flash Free"}]}]}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                let event = &events[0];
                assert_eq!(event.kind, RuntimeEventKind::SessionStarted);
                assert_eq!(event.external_session_id.as_deref(), Some("ses_x"));
                let model = &event.metadata["acp_model_option"];
                assert_eq!(model["config_id"], "model");
                assert_eq!(model["current"], "opencode/big-pickle");
                assert_eq!(model["options"][1]["value"], "opencode/deepseek-v4-flash-free");
                assert_eq!(model["options"][1]["name"], "DeepSeek Flash Free");
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn session_new_without_config_options_has_no_model_option() {
        let line = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"gem-1"}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                assert_eq!(events[0].external_session_id.as_deref(), Some("gem-1"));
                assert!(events[0].metadata.get("acp_model_option").is_none());
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn set_config_option_request_shape() {
        let request = build_acp_set_config_option_request(9, "ses_x", "model", "opencode/deepseek-v4-flash-free");
        assert_eq!(request["method"], "session/set_config_option");
        assert_eq!(request["params"]["sessionId"], "ses_x");
        assert_eq!(request["params"]["configId"], "model");
        assert_eq!(request["params"]["value"], "opencode/deepseek-v4-flash-free");
    }

    #[test]
    fn prompt_response_yields_turn_completed() {
        let line = r#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                let event = &events[0];
                assert_eq!(event.kind, RuntimeEventKind::TurnCompleted);
                assert_eq!(event.text.as_deref(), Some("end_turn"));
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn error_response_surfaces_message() {
        let line = r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32000,"message":"boom"}}"#;
        match classify_acp_message(line).expect("classify") {
            AcpInbound::Emit(events) => {
                assert_eq!(events[0].kind, RuntimeEventKind::Error);
                assert_eq!(events[0].text.as_deref(), Some("boom"));
            }
            other => panic!("expected Emit, got {other:?}"),
        }
    }

    #[test]
    fn permission_option_selection_prefers_remembered_choice() {
        let options = json!([
            { "optionId": "once-allow", "kind": "allow_once" },
            { "optionId": "always-allow", "kind": "allow_always" },
            { "optionId": "once-reject", "kind": "reject_once" }
        ]);
        assert_eq!(
            select_permission_option(&options, true, false).as_deref(),
            Some("once-allow")
        );
        assert_eq!(
            select_permission_option(&options, true, true).as_deref(),
            Some("always-allow")
        );
        assert_eq!(
            select_permission_option(&options, false, false).as_deref(),
            Some("once-reject")
        );
        // No reject_always offered: falls back to reject_once.
        assert_eq!(
            select_permission_option(&options, false, true).as_deref(),
            Some("once-reject")
        );
    }

    #[test]
    fn request_id_round_trips_through_string() {
        assert_eq!(request_id_from_string("42"), Value::from(42));
        assert_eq!(request_id_from_string("w1"), Value::from("w1"));
        assert_eq!(json_id_to_string(&Value::from(42)), "42");
        assert_eq!(json_id_to_string(&Value::from("w1")), "w1");
    }

    #[test]
    fn workspace_path_allows_paths_inside_root() {
        let resolved = resolve_workspace_path("D:/work/project", "src/main.rs").expect("inside");
        assert!(resolved.starts_with(normalize(Path::new("D:/work/project"))));
    }

    #[test]
    fn workspace_path_rejects_escape_via_parent_dir() {
        let error = resolve_workspace_path("D:/work/project", "../secret.txt").unwrap_err();
        assert!(error.contains("越界"));
    }

    #[test]
    fn workspace_path_rejects_absolute_outside_root() {
        let error = resolve_workspace_path("D:/work/project", "D:/other/file.txt").unwrap_err();
        assert!(error.contains("越界"));
    }
}
