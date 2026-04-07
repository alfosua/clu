use clu_ai::OpenAICompatible;
use clu_core::protocol::{Command, Event, RpcOutput, RpcResponse};
use clu_core::session::AgentSession;
use clu_core::Settings;
use clu_tools::{BashTool, EditTool, PowerShellTool, ReadTool, WriteTool};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx_cmd, rx_cmd) = mpsc::channel::<Command>(100);
    let (tx_evt, mut rx_evt) = mpsc::channel::<Event>(8192);
    let (tx_out, mut rx_out) = mpsc::channel::<RpcOutput>(8192);

    let settings = Settings::load();
    let base_url = settings.effective_base_url("http://localhost:11434/v1");
    let api_key = settings.effective_api_key("ollama");
    let model = settings.effective_model("llama3.2");
    let provider = Arc::new(OpenAICompatible::new(base_url, api_key));
    let mut session = AgentSession::new(provider, model);
    if let Some(sp) = settings.system_prompt.clone() {
        session.set_system_prompt(sp);
    }
    let enabled: Option<std::collections::HashSet<String>> = std::env::var("CLU_TOOLS")
        .ok()
        .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect());
    let want = |name: &str| enabled.as_ref().map_or(true, |s| s.contains(name));
    if want("bash") {
        session.register_tool(Arc::new(BashTool));
    }
    if want("pwsh") {
        session.register_tool(Arc::new(PowerShellTool));
    }
    if want("read") {
        session.register_tool(Arc::new(ReadTool));
    }
    if want("write") {
        session.register_tool(Arc::new(WriteTool));
    }
    if want("edit") {
        session.register_tool(Arc::new(EditTool));
    }

    // Forward events → output channel
    let tx_out_events = tx_out.clone();
    tokio::spawn(async move {
        while let Some(evt) = rx_evt.recv().await {
            let _ = tx_out_events.send(RpcOutput::Event(evt)).await;
        }
    });

    // Run the core engine
    tokio::spawn(async move {
        session.run(rx_cmd, tx_evt).await;
    });

    // Stdout writer
    let stdout_handle = tokio::spawn(async move {
        let mut stdout = io::stdout();
        while let Some(first) = rx_out.recv().await {
            let mut buf = Vec::with_capacity(4096);
            if let Ok(json) = serde_json::to_string(&first) {
                buf.extend_from_slice(json.as_bytes());
                buf.push(b'\n');
            }
            // Drain anything else already queued so we flush once per batch.
            while let Ok(out) = rx_out.try_recv() {
                if let Ok(json) = serde_json::to_string(&out) {
                    buf.extend_from_slice(json.as_bytes());
                    buf.push(b'\n');
                }
            }
            let _ = stdout.write_all(&buf).await;
            let _ = stdout.flush().await;
        }
    });

    // Stdin reader
    //
    // Protocol: strict LF-delimited JSONL.
    // Each line is a JSON object. The optional top-level `"id"` field is extracted
    // first for request/response correlation; the rest is deserialized as a `Command`.
    let stdin_handle = tokio::spawn(async move {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin).lines();

        while let Ok(Some(line)) = reader.next_line().await {
            let line = line.trim_end_matches('\r').to_owned(); // tolerate CRLF input
            if line.is_empty() {
                continue;
            }

            // Extract the optional `id` before full deserialization.
            let raw: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx_out
                        .send(RpcOutput::Response(RpcResponse::err(
                            "parse",
                            None,
                            format!("Failed to parse JSON: {e}"),
                        )))
                        .await;
                    continue;
                }
            };

            let id = raw
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_owned);

            let cmd: Command = match serde_json::from_value(raw) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx_out
                        .send(RpcOutput::Response(RpcResponse::err(
                            "parse",
                            id,
                            format!("Unknown or malformed command: {e}"),
                        )))
                        .await;
                    continue;
                }
            };

            dispatch(cmd, id, &tx_cmd, &tx_out).await;
        }
    });

    let _ = tokio::join!(stdout_handle, stdin_handle);
    Ok(())
}

/// Route a parsed command: send an immediate `RpcResponse` ack and, when needed,
/// forward the command to the agent session for async processing.
async fn dispatch(
    cmd: Command,
    id: Option<String>,
    tx_cmd: &mpsc::Sender<Command>,
    tx_out: &mpsc::Sender<RpcOutput>,
) {
    match &cmd {
        // ── Async prompting — ack immediately, events follow ───────────────
        Command::Prompt { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("prompt", id)))
                .await;
            let _ = tx_cmd.send(cmd).await;
        }
        Command::Steer { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("steer", id)))
                .await;
            let _ = tx_cmd.send(cmd).await;
        }
        Command::FollowUp { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("follow_up", id)))
                .await;
            let _ = tx_cmd.send(cmd).await;
        }

        // ── Synchronous-style commands (forwarded + acked) ─────────────────
        Command::Abort => {
            let _ = tx_cmd.send(cmd).await;
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("abort", id)))
                .await;
        }
        Command::SetModel { .. } => {
            let _ = tx_cmd.send(cmd).await;
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_model", id)))
                .await;
        }
        Command::SetThinkingLevel { .. } => {
            let _ = tx_cmd.send(cmd).await;
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_thinking_level", id)))
                .await;
        }
        Command::SetSteeringMode { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_steering_mode", id)))
                .await;
        }
        Command::SetFollowUpMode { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_follow_up_mode", id)))
                .await;
        }
        Command::SetAutoCompaction { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_auto_compaction", id)))
                .await;
        }
        Command::SetAutoRetry { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_auto_retry", id)))
                .await;
        }
        Command::AbortRetry => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("abort_retry", id)))
                .await;
        }
        Command::AbortBash => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("abort_bash", id)))
                .await;
        }
        Command::SetSessionName { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("set_session_name", id)))
                .await;
        }
        Command::NewSession { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "new_session",
                    id,
                    serde_json::json!({"cancelled": false}),
                )))
                .await;
        }

        // ── State / query commands (stub responses) ────────────────────────
        Command::GetState => {
            // TODO: return real session state
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("get_state", id)))
                .await;
        }
        Command::GetMessages => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "get_messages",
                    id,
                    serde_json::json!({"messages": []}),
                )))
                .await;
        }
        Command::GetAvailableModels => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "get_available_models",
                    id,
                    serde_json::json!({"models": []}),
                )))
                .await;
        }
        Command::CycleModel => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "cycle_model",
                    id,
                    serde_json::Value::Null,
                )))
                .await;
        }
        Command::CycleThinkingLevel => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "cycle_thinking_level",
                    id,
                    serde_json::Value::Null,
                )))
                .await;
        }
        Command::GetSessionStats => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("get_session_stats", id)))
                .await;
        }
        Command::GetForkMessages => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "get_fork_messages",
                    id,
                    serde_json::json!({"messages": []}),
                )))
                .await;
        }
        Command::GetLastAssistantText => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "get_last_assistant_text",
                    id,
                    serde_json::json!({"text": null}),
                )))
                .await;
        }
        Command::GetCommands => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok_with_data(
                    "get_commands",
                    id,
                    serde_json::json!({"commands": []}),
                )))
                .await;
        }

        // ── Bash (forwarded to session) ────────────────────────────────────
        Command::Bash { .. } => {
            let _ = tx_cmd.send(cmd).await;
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::ok("bash", id)))
                .await;
        }

        // ── Session management stubs ───────────────────────────────────────
        Command::Compact { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::err(
                    "compact",
                    id,
                    "not yet implemented",
                )))
                .await;
        }
        Command::ExportHtml { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::err(
                    "export_html",
                    id,
                    "not yet implemented",
                )))
                .await;
        }
        Command::SwitchSession { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::err(
                    "switch_session",
                    id,
                    "not yet implemented",
                )))
                .await;
        }
        Command::Fork { .. } => {
            let _ = tx_out
                .send(RpcOutput::Response(RpcResponse::err(
                    "fork",
                    id,
                    "not yet implemented",
                )))
                .await;
        }
    }
}
