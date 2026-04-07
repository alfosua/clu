/// RPC protocol types matching pi-coding-agent's protocol.
///
/// Commands are received as JSON lines on stdin.
/// Responses (correlated to commands via optional `id`) and Events (unsolicited) are
/// emitted as JSON lines on stdout.
///
/// Naming conventions:
/// - JSON type discriminants (the `"type"` field): snake_case   (e.g. `"agent_start"`)
/// - JSON field names: camelCase                                  (e.g. `"toolCallId"`)
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// Scalar / enum types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueueMode {
    All,
    OneAtATime,
}

/// Behaviour when sending a prompt while the agent is already streaming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamingBehavior {
    /// Deliver after the current assistant turn finishes executing its tool calls.
    Steer,
    /// Deliver only after the agent finishes all work.
    FollowUp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionReason {
    Manual,
    Threshold,
    Overflow,
}

// ============================================================================
// Content block types
// ============================================================================

/// A block inside an assistant message's content array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AssistantContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "thinking")]
    Thinking { thinking: String },

    /// A tool-call request emitted by the assistant.
    #[serde(rename = "toolCall")]
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
}

/// A block inside a user message's content array (when content is not a plain string).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UserContentPart {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// Image attachment used inside commands (prompt / steer / follow_up).
/// Always has `type: "image"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageAttachment {
    #[serde(rename = "type")]
    pub type_: String,
    pub data: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

impl ImageAttachment {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            type_: "image".to_string(),
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }
}

// ============================================================================
// Usage / cost
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    pub cost: TokenCost,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
            cost: TokenCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                total: 0.0,
            },
        }
    }
}

// ============================================================================
// Message types
// ============================================================================

/// User message content: either a plain string or a mixed array of text/image parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Parts(Vec<UserContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    pub content: UserContent,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub content: Vec<AssistantContentBlock>,
    pub api: String,
    pub provider: String,
    pub model: String,
    pub usage: Usage,
    pub stop_reason: StopReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<UserContentPart>,
    pub is_error: bool,
    pub timestamp: u64,
}

/// Union of all message types that appear in the conversation transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum AgentMessage {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "toolResult")]
    ToolResult(ToolResultMessage),
}

// ============================================================================
// Model descriptor
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    pub base_url: String,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: u64,
    pub max_tokens: u64,
    pub cost: ModelCost,
}

// ============================================================================
// AssistantMessageEvent — streaming delta events
// ============================================================================

/// Streaming delta events emitted inside a `message_update` event.
/// Each delta also carries the current accumulated partial `AssistantMessage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantMessageEvent {
    /// Message generation started.
    Start { partial: AssistantMessage },

    /// A text content block started.
    TextStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: AssistantMessage,
    },
    /// A text chunk arrived.
    TextDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    /// A text content block ended; `content` is the full accumulated text.
    TextEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },

    /// A thinking block started.
    ThinkingStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: AssistantMessage,
    },
    /// A thinking chunk arrived.
    ThinkingDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    /// A thinking block ended.
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },

    /// A tool-call block started.
    ToolcallStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: AssistantMessage,
    },
    /// A tool-call arguments chunk arrived.
    ToolcallDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    /// A tool-call block ended; `tool_call` is the full resolved call.
    ToolcallEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        #[serde(rename = "toolCall")]
        tool_call: AssistantContentBlock,
        partial: AssistantMessage,
    },

    /// Message generation completed successfully.
    Done {
        reason: StopReason,
        message: AssistantMessage,
    },
    /// Message generation ended with an error or was aborted.
    Error {
        reason: StopReason,
        error: AssistantMessage,
    },
}

// ============================================================================
// Commands (stdin)
// ============================================================================

/// All commands the client can send on stdin.
/// Each variant maps to a `"type"` string in the JSON.
/// Commands may include an optional `"id"` field (parsed separately in the RPC layer).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // ── Prompting ──────────────────────────────────────────────────────────
    Prompt {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageAttachment>>,
        #[serde(rename = "streamingBehavior", default, skip_serializing_if = "Option::is_none")]
        streaming_behavior: Option<StreamingBehavior>,
    },
    Steer {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageAttachment>>,
    },
    FollowUp {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageAttachment>>,
    },
    Abort,
    NewSession {
        #[serde(rename = "parentSession", default, skip_serializing_if = "Option::is_none")]
        parent_session: Option<String>,
    },

    // ── State ──────────────────────────────────────────────────────────────
    GetState,
    GetMessages,

    // ── Model ──────────────────────────────────────────────────────────────
    SetModel {
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },
    CycleModel,
    GetAvailableModels,

    // ── Thinking ───────────────────────────────────────────────────────────
    SetThinkingLevel { level: ThinkingLevel },
    CycleThinkingLevel,

    // ── Queue modes ────────────────────────────────────────────────────────
    SetSteeringMode { mode: QueueMode },
    SetFollowUpMode { mode: QueueMode },

    // ── Compaction ─────────────────────────────────────────────────────────
    Compact {
        #[serde(rename = "customInstructions", default, skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },
    SetAutoCompaction { enabled: bool },

    // ── Retry ──────────────────────────────────────────────────────────────
    SetAutoRetry { enabled: bool },
    AbortRetry,

    // ── Bash ───────────────────────────────────────────────────────────────
    Bash { command: String },
    AbortBash,

    // ── Session ────────────────────────────────────────────────────────────
    GetSessionStats,
    ExportHtml {
        #[serde(rename = "outputPath", default, skip_serializing_if = "Option::is_none")]
        output_path: Option<String>,
    },
    SwitchSession {
        #[serde(rename = "sessionPath")]
        session_path: String,
    },
    Fork {
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    GetForkMessages,
    GetLastAssistantText,
    SetSessionName { name: String },

    // ── Commands list ──────────────────────────────────────────────────────
    GetCommands,
}

// ============================================================================
// Events (stdout — unsolicited)
// ============================================================================

/// Unsolicited events streamed to stdout during agent operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    // ── Agent lifecycle ────────────────────────────────────────────────────
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },

    // ── Turn lifecycle ─────────────────────────────────────────────────────
    TurnStart,
    TurnEnd {
        message: AgentMessage,
        #[serde(rename = "toolResults")]
        tool_results: Vec<ToolResultMessage>,
    },

    // ── Message lifecycle ──────────────────────────────────────────────────
    MessageStart {
        message: AgentMessage,
    },
    /// Streaming update; emitted only for assistant messages.
    MessageUpdate {
        message: AgentMessage,
        #[serde(rename = "assistantMessageEvent")]
        assistant_message_event: AssistantMessageEvent,
    },
    MessageEnd {
        message: AgentMessage,
    },

    // ── Tool execution ─────────────────────────────────────────────────────
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: Value,
    },
    /// Carries accumulated (not delta) partial output so clients can replace their display.
    ToolExecutionUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: Value,
        #[serde(rename = "partialResult")]
        partial_result: Value,
    },
    ToolExecutionEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: Value,
        #[serde(rename = "isError")]
        is_error: bool,
    },

    // ── Queue ──────────────────────────────────────────────────────────────
    /// Emitted whenever the pending steering or follow-up queue changes.
    QueueUpdate {
        steering: Vec<String>,
        #[serde(rename = "followUp")]
        follow_up: Vec<String>,
    },

    // ── Compaction ─────────────────────────────────────────────────────────
    CompactionStart {
        reason: CompactionReason,
    },
    CompactionEnd {
        reason: CompactionReason,
        result: Option<Value>,
        aborted: bool,
        #[serde(rename = "willRetry")]
        will_retry: bool,
        #[serde(rename = "errorMessage", skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
    },

    // ── Auto-retry ─────────────────────────────────────────────────────────
    AutoRetryStart {
        attempt: u32,
        #[serde(rename = "maxAttempts")]
        max_attempts: u32,
        #[serde(rename = "delayMs")]
        delay_ms: u64,
        #[serde(rename = "errorMessage")]
        error_message: String,
    },
    AutoRetryEnd {
        success: bool,
        attempt: u32,
        #[serde(rename = "finalError", skip_serializing_if = "Option::is_none")]
        final_error: Option<String>,
    },

    // ── Extensions ─────────────────────────────────────────────────────────
    ExtensionError {
        #[serde(rename = "extensionPath")]
        extension_path: String,
        event: String,
        error: String,
    },
}

// ============================================================================
// RPC Response (stdout — correlated to a command via id)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct RpcResponse {
    /// Always `"response"`.
    #[serde(rename = "type")]
    pub type_: String,
    /// The command type string this response is for (e.g. `"prompt"`).
    pub command: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RpcResponse {
    pub fn ok(command: impl Into<String>, id: Option<String>) -> Self {
        Self {
            type_: "response".into(),
            command: command.into(),
            success: true,
            id,
            data: None,
            error: None,
        }
    }

    pub fn ok_with_data(command: impl Into<String>, id: Option<String>, data: Value) -> Self {
        Self {
            type_: "response".into(),
            command: command.into(),
            success: true,
            id,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(command: impl Into<String>, id: Option<String>, error: impl Into<String>) -> Self {
        Self {
            type_: "response".into(),
            command: command.into(),
            success: false,
            id,
            data: None,
            error: Some(error.into()),
        }
    }
}

// ============================================================================
// RPC Output (stdout)
// ============================================================================

/// Every line written to stdout is either a correlated `Response` or an unsolicited `Event`.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RpcOutput {
    Response(RpcResponse),
    Event(Event),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // ── Command round-trips ────────────────────────────────────────────────

    #[test]
    fn command_prompt_serializes() {
        let cmd = Command::Prompt {
            message: "Hello world".into(),
            images: None,
            streaming_behavior: None,
        };
        let v: Value = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v["type"], "prompt");
        assert_eq!(v["message"], "Hello world");
        // Optional fields should be absent
        assert!(v.get("images").is_none());
        assert!(v.get("streamingBehavior").is_none());
    }

    #[test]
    fn command_prompt_deserializes_with_id() {
        // The "id" field from the wire is not part of Command; it's extracted separately.
        // Command deserializer must silently ignore unknown fields.
        let json_str = r#"{"id":"req-1","type":"prompt","message":"Hi"}"#;
        let cmd: Command = serde_json::from_str(json_str).unwrap();
        assert!(matches!(cmd, Command::Prompt { ref message, .. } if message == "Hi"));
    }

    #[test]
    fn command_set_model_camel_case() {
        let json_str = r#"{"type":"set_model","provider":"openai","modelId":"gpt-4o"}"#;
        let cmd: Command = serde_json::from_str(json_str).unwrap();
        match cmd {
            Command::SetModel { provider, model_id } => {
                assert_eq!(provider, "openai");
                assert_eq!(model_id, "gpt-4o");
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn command_streaming_behavior_camel_case() {
        let json_str = r#"{"type":"prompt","message":"go","streamingBehavior":"followUp"}"#;
        let cmd: Command = serde_json::from_str(json_str).unwrap();
        match cmd {
            Command::Prompt { streaming_behavior: Some(b), .. } => {
                assert_eq!(b, StreamingBehavior::FollowUp);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn command_abort_unit_variant() {
        let json_str = r#"{"type":"abort"}"#;
        let cmd: Command = serde_json::from_str(json_str).unwrap();
        assert!(matches!(cmd, Command::Abort));

        let serialized = serde_json::to_string(&cmd).unwrap();
        assert_eq!(serialized, r#"{"type":"abort"}"#);
    }

    // ── RpcResponse ────────────────────────────────────────────────────────

    #[test]
    fn rpc_response_ok_shape() {
        let resp = RpcResponse::ok("prompt", Some("req-1".into()));
        let v: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["type"], "response");
        assert_eq!(v["command"], "prompt");
        assert_eq!(v["success"], true);
        assert_eq!(v["id"], "req-1");
        assert!(v.get("data").is_none());
        assert!(v.get("error").is_none());
    }

    #[test]
    fn rpc_response_err_shape() {
        let resp = RpcResponse::err("set_model", None, "model not found");
        let v: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["type"], "response");
        assert_eq!(v["success"], false);
        assert_eq!(v["error"], "model not found");
        assert!(v.get("id").is_none());
    }

    // ── Events ────────────────────────────────────────────────────────────

    #[test]
    fn event_agent_start_shape() {
        let evt = Event::AgentStart;
        let v: Value = serde_json::to_value(&evt).unwrap();
        assert_eq!(v, json!({"type": "agent_start"}));
    }

    #[test]
    fn event_tool_execution_start_camel_case() {
        let evt = Event::ToolExecutionStart {
            tool_call_id: "call_123".into(),
            tool_name: "bash".into(),
            args: json!({"command": "ls"}),
        };
        let v: Value = serde_json::to_value(&evt).unwrap();
        assert_eq!(v["type"], "tool_execution_start");
        assert_eq!(v["toolCallId"], "call_123");
        assert_eq!(v["toolName"], "bash");
    }

    #[test]
    fn event_message_update_shape() {
        let msg = AssistantMessage {
            content: vec![AssistantContentBlock::Text { text: "hi".into() }],
            api: "openai-completions".into(),
            provider: "openai".into(),
            model: "gpt-4o".into(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        };
        let delta = AssistantMessageEvent::TextDelta {
            content_index: 0,
            delta: "hi".into(),
            partial: msg.clone(),
        };
        let evt = Event::MessageUpdate {
            message: AgentMessage::Assistant(msg),
            assistant_message_event: delta,
        };
        let v: Value = serde_json::to_value(&evt).unwrap();
        assert_eq!(v["type"], "message_update");
        assert_eq!(v["assistantMessageEvent"]["type"], "text_delta");
        assert_eq!(v["assistantMessageEvent"]["contentIndex"], 0);
        assert_eq!(v["assistantMessageEvent"]["delta"], "hi");
    }

    // ── AgentMessage round-trip ────────────────────────────────────────────

    #[test]
    fn agent_message_user_role_tag() {
        let msg = AgentMessage::User(UserMessage {
            content: UserContent::Text("hello".into()),
            timestamp: 1234567890,
        });
        let v: Value = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], "hello");
        assert_eq!(v["timestamp"], 1234567890u64);
    }

    #[test]
    fn agent_message_assistant_role_tag() {
        let msg = AgentMessage::Assistant(AssistantMessage {
            content: vec![AssistantContentBlock::Text { text: "ok".into() }],
            api: "openai-completions".into(),
            provider: "openai".into(),
            model: "gpt-4o".into(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        });
        let v: Value = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "assistant");
        assert_eq!(v["stopReason"], "stop");
        assert_eq!(v["content"][0]["type"], "text");
        assert_eq!(v["content"][0]["text"], "ok");
    }

    #[test]
    fn agent_message_tool_result_role_tag() {
        let msg = AgentMessage::ToolResult(ToolResultMessage {
            tool_call_id: "call_abc".into(),
            tool_name: "bash".into(),
            content: vec![UserContentPart::Text { text: "output".into() }],
            is_error: false,
            timestamp: 0,
        });
        let v: Value = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "toolResult");
        assert_eq!(v["toolCallId"], "call_abc");
        assert_eq!(v["toolName"], "bash");
        assert_eq!(v["isError"], false);
    }

    // ── RpcOutput untagged union ───────────────────────────────────────────

    #[test]
    fn rpc_output_response_vs_event() {
        let resp = RpcOutput::Response(RpcResponse::ok("abort", None));
        let r: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(r["type"], "response");

        let evt = RpcOutput::Event(Event::AgentStart);
        let e: Value = serde_json::to_value(&evt).unwrap();
        assert_eq!(e["type"], "agent_start");
    }

    // ── AssistantContentBlock ─────────────────────────────────────────────

    #[test]
    fn assistant_content_block_tool_call() {
        let block = AssistantContentBlock::ToolCall {
            id: "call_1".into(),
            name: "bash".into(),
            arguments: json!({"command": "ls"}),
        };
        let v: Value = serde_json::to_value(&block).unwrap();
        assert_eq!(v["type"], "toolCall");
        assert_eq!(v["id"], "call_1");
        assert_eq!(v["name"], "bash");
    }

    // ── ThinkingLevel / QueueMode casing ──────────────────────────────────

    #[test]
    fn thinking_level_snake_case() {
        assert_eq!(serde_json::to_string(&ThinkingLevel::Xhigh).unwrap(), r#""xhigh""#);
        assert_eq!(serde_json::to_string(&ThinkingLevel::Off).unwrap(), r#""off""#);
    }

    #[test]
    fn queue_mode_kebab_case() {
        assert_eq!(
            serde_json::to_string(&QueueMode::OneAtATime).unwrap(),
            r#""one-at-a-time""#
        );
        assert_eq!(serde_json::to_string(&QueueMode::All).unwrap(), r#""all""#);
    }
}
