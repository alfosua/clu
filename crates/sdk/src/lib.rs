//! # clu-sdk
//!
//! Rust client for the `clu-rpc` headless agent process. Spawns `clu-rpc` as
//! a subprocess and exposes a typed async API over its JSONL protocol.
//!
//! This crate is the Rust counterpart of pi-coding-agent's `rpc-client.ts`.
//! For non-Rust languages, talk to `clu-rpc` directly over stdin/stdout using
//! the same wire protocol (see `clu-core::protocol`).
//!
//! ## Quick start
//!
//! ```no_run
//! use clu_sdk::CluClient;
//!
//! # async fn demo() -> Result<(), clu_sdk::SdkError> {
//! let client = CluClient::spawn().await?;
//!
//! // Fire-and-forget style: subscribe to events, then send a prompt.
//! let mut events = client.events();
//! client.prompt("Hello, world!").await?;
//!
//! while let Ok(evt) = events.recv().await {
//!     println!("{evt:?}");
//! }
//! # Ok(()) }
//! ```
//!
//! ## Higher-level prompt stream
//!
//! ```no_run
//! use clu_sdk::CluClient;
//! use clu_core::protocol::Event;
//!
//! # async fn demo() -> Result<(), clu_sdk::SdkError> {
//! let client = CluClient::spawn().await?;
//! let mut stream = client.run_prompt("What's 2 + 2?").await?;
//! while let Some(evt) = stream.next().await {
//!     if let Event::MessageUpdate { .. } = evt {
//!         // handle streaming text deltas
//!     }
//! }
//! # Ok(()) }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clu_core::protocol::{
    AgentMessage, Command, Event, Model, QueueMode, RpcResponse, ThinkingLevel,
};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command as TokioCommand};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON (de)serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("the clu-rpc subprocess has exited")]
    ProcessExited,

    #[error("request failed: {0}")]
    RequestFailed(String),

    #[error("internal channel closed")]
    ChannelClosed,
}

pub type Result<T> = std::result::Result<T, SdkError>;

// ============================================================================
// Internal types
// ============================================================================

/// A pending command + its optional response slot, waiting to be written to
/// the child's stdin.
struct Outgoing {
    id: String,
    command: Command,
    /// If set, the reader task will forward the matching response here.
    response: Option<oneshot::Sender<RpcResponse>>,
}

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<RpcResponse>>>>;

// ============================================================================
// CluClient
// ============================================================================

/// An async client for a running `clu-rpc` subprocess.
///
/// Cloning a `CluClient` is cheap — all clones share the same subprocess.
/// When the last `CluClient` is dropped, the subprocess is killed.
#[derive(Clone)]
pub struct CluClient {
    inner: Arc<Inner>,
}

struct Inner {
    tx_cmd: mpsc::Sender<Outgoing>,
    evt_tx: broadcast::Sender<Event>,
    next_id: AtomicU64,
    /// Owns the subprocess handle so dropping the client kills it.
    _child: Mutex<Child>,
}

impl CluClient {
    /// Spawn `clu-rpc` from the `PATH` and connect to it.
    pub async fn spawn() -> Result<Self> {
        Self::spawn_with_path(default_rpc_path()).await
    }

    /// Spawn a specific `clu-rpc` binary (or any binary that implements the
    /// same JSONL protocol) and connect to it.
    pub async fn spawn_with_path(path: impl AsRef<Path>) -> Result<Self> {
        let mut child = TokioCommand::new(path.as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            SdkError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "failed to capture clu-rpc stdin",
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            SdkError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "failed to capture clu-rpc stdout",
            ))
        })?;

        let (tx_cmd, rx_cmd) = mpsc::channel::<Outgoing>(100);
        let (evt_tx, _) = broadcast::channel::<Event>(256);

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

        // Writer task: drains commands, serializes them, injects `id`, writes to stdin.
        tokio::spawn(writer_task(stdin, rx_cmd, pending.clone()));

        // Reader task: parses each line, routes responses + events.
        tokio::spawn(reader_task(stdout, pending, evt_tx.clone()));

        Ok(Self {
            inner: Arc::new(Inner {
                tx_cmd,
                evt_tx,
                next_id: AtomicU64::new(1),
                _child: Mutex::new(child),
            }),
        })
    }

    // ── Event subscription ─────────────────────────────────────────────────

    /// Subscribe to the stream of all agent `Event`s. Each subscriber gets
    /// its own receiver; late subscribers do not see events emitted before
    /// they subscribed.
    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.inner.evt_tx.subscribe()
    }

    // ── Generic request/response ───────────────────────────────────────────

    /// Send any command and await its correlated `RpcResponse`.
    pub async fn request(&self, command: Command) -> Result<RpcResponse> {
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();

        self.inner
            .tx_cmd
            .send(Outgoing {
                id,
                command,
                response: Some(tx),
            })
            .await
            .map_err(|_| SdkError::ProcessExited)?;

        rx.await.map_err(|_| SdkError::ChannelClosed)
    }

    /// Send a command without awaiting its response.
    pub async fn send(&self, command: Command) -> Result<()> {
        let id = self.next_id();
        self.inner
            .tx_cmd
            .send(Outgoing {
                id,
                command,
                response: None,
            })
            .await
            .map_err(|_| SdkError::ProcessExited)
    }

    fn next_id(&self) -> String {
        let n = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        format!("req-{n}")
    }

    // ── Typed convenience methods ──────────────────────────────────────────

    /// Send a user prompt. Returns the immediate acknowledgement; the actual
    /// agent work streams back as events. Use [`CluClient::events`] or
    /// [`CluClient::run_prompt`] to consume them.
    pub async fn prompt(&self, message: impl Into<String>) -> Result<RpcResponse> {
        self.request(Command::Prompt {
            message: message.into(),
            images: None,
            streaming_behavior: None,
        })
        .await
        .and_then(ensure_ok)
    }

    /// Abort the current agent operation.
    pub async fn abort(&self) -> Result<()> {
        self.request(Command::Abort).await.and_then(ensure_ok)?;
        Ok(())
    }

    /// Switch to a specific model.
    pub async fn set_model(
        &self,
        provider: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Option<Model>> {
        let resp = self
            .request(Command::SetModel {
                provider: provider.into(),
                model_id: model_id.into(),
            })
            .await
            .and_then(ensure_ok)?;

        Ok(resp.data.and_then(|v| serde_json::from_value(v).ok()))
    }

    /// Set the reasoning/thinking level.
    pub async fn set_thinking_level(&self, level: ThinkingLevel) -> Result<()> {
        self.request(Command::SetThinkingLevel { level })
            .await
            .and_then(ensure_ok)?;
        Ok(())
    }

    /// Set the steering-message delivery mode.
    pub async fn set_steering_mode(&self, mode: QueueMode) -> Result<()> {
        self.request(Command::SetSteeringMode { mode })
            .await
            .and_then(ensure_ok)?;
        Ok(())
    }

    /// Set the follow-up message delivery mode.
    pub async fn set_follow_up_mode(&self, mode: QueueMode) -> Result<()> {
        self.request(Command::SetFollowUpMode { mode })
            .await
            .and_then(ensure_ok)?;
        Ok(())
    }

    /// Get all messages in the current session.
    pub async fn get_messages(&self) -> Result<Vec<AgentMessage>> {
        let resp = self
            .request(Command::GetMessages)
            .await
            .and_then(ensure_ok)?;

        let data = resp.data.unwrap_or(Value::Null);
        let msgs = data
            .get("messages")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        Ok(serde_json::from_value(msgs)?)
    }

    /// Get the current raw session state as a JSON value. (A typed
    /// `RpcSessionState` will land once the core tracks state.)
    pub async fn get_state(&self) -> Result<Value> {
        let resp = self.request(Command::GetState).await.and_then(ensure_ok)?;
        Ok(resp.data.unwrap_or(Value::Null))
    }

    /// Execute a bash command and return the raw result data.
    pub async fn bash(&self, command: impl Into<String>) -> Result<Value> {
        let resp = self
            .request(Command::Bash {
                command: command.into(),
            })
            .await
            .and_then(ensure_ok)?;
        Ok(resp.data.unwrap_or(Value::Null))
    }

    /// Get the list of commands (extension commands, prompt templates, skills).
    pub async fn get_commands(&self) -> Result<Value> {
        let resp = self
            .request(Command::GetCommands)
            .await
            .and_then(ensure_ok)?;
        Ok(resp.data.unwrap_or(Value::Null))
    }

    // ── High-level prompt stream ───────────────────────────────────────────

    /// Send a prompt and return a [`PromptStream`] that yields every event
    /// belonging to the resulting agent run, up to and including `agent_end`.
    ///
    /// Internally this subscribes to the event channel *before* sending the
    /// prompt, so no events can be missed.
    pub async fn run_prompt(&self, message: impl Into<String>) -> Result<PromptStream> {
        let rx = self.events();
        self.prompt(message).await?;
        Ok(PromptStream {
            rx,
            started: false,
            finished: false,
        })
    }
}

// ============================================================================
// PromptStream
// ============================================================================

/// An event stream scoped to a single `prompt` call. Yields events from the
/// next `agent_start` up to (and including) the matching `agent_end`, then
/// terminates.
pub struct PromptStream {
    rx: broadcast::Receiver<Event>,
    started: bool,
    finished: bool,
}

impl PromptStream {
    /// Await the next event in this run, or `None` if the run is finished
    /// or the underlying channel was closed.
    pub async fn next(&mut self) -> Option<Event> {
        if self.finished {
            return None;
        }

        loop {
            let evt = match self.rx.recv().await {
                Ok(e) => e,
                Err(_) => {
                    // Lagged or closed — terminate the stream.
                    self.finished = true;
                    return None;
                }
            };

            if !self.started {
                // Drop anything from previous runs until we see an agent_start.
                if matches!(evt, Event::AgentStart) {
                    self.started = true;
                    return Some(evt);
                } else {
                    continue;
                }
            }

            let is_end = matches!(evt, Event::AgentEnd { .. });
            if is_end {
                self.finished = true;
            }
            return Some(evt);
        }
    }
}

// ============================================================================
// Writer / reader tasks
// ============================================================================

async fn writer_task(
    mut stdin: ChildStdin,
    mut rx_cmd: mpsc::Receiver<Outgoing>,
    pending: PendingMap,
) {
    while let Some(out) = rx_cmd.recv().await {
        // Register the oneshot slot first so a response that arrives before
        // `write_all` returns still finds it.
        if let Some(tx) = out.response {
            pending.lock().await.insert(out.id.clone(), tx);
        }

        // Serialize the command, then inject the `id` field at the top level.
        let mut val = match serde_json::to_value(&out.command) {
            Ok(v) => v,
            Err(_) => {
                pending.lock().await.remove(&out.id);
                continue;
            }
        };
        if let Value::Object(map) = &mut val {
            map.insert("id".to_string(), Value::String(out.id.clone()));
        }

        let mut line = val.to_string();
        line.push('\n');

        if stdin.write_all(line.as_bytes()).await.is_err() {
            pending.lock().await.remove(&out.id);
            break;
        }
        if stdin.flush().await.is_err() {
            pending.lock().await.remove(&out.id);
            break;
        }
    }
}

async fn reader_task(
    stdout: tokio::process::ChildStdout,
    pending: PendingMap,
    evt_tx: broadcast::Sender<Event>,
) {
    let mut reader = BufReader::new(stdout).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // RpcResponse detection: `{"type":"response", ...}`.
        let is_response = value
            .get("type")
            .and_then(Value::as_str)
            .map(|t| t == "response")
            .unwrap_or(false);

        if is_response {
            // Parse as RpcResponse manually (it's Serialize-only in core, so we
            // reconstruct the fields we need from the Value).
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let command = value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let success = value
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let error = value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string);
            let data = value.get("data").cloned();

            let resp = RpcResponse {
                type_: "response".to_string(),
                command,
                success,
                id: id.clone(),
                data,
                error,
            };

            if let Some(id) = id {
                if let Some(tx) = pending.lock().await.remove(&id) {
                    let _ = tx.send(resp);
                }
            }
            continue;
        }

        // Otherwise: try to parse as an Event and broadcast it.
        if let Ok(event) = serde_json::from_value::<Event>(value) {
            let _ = evt_tx.send(event);
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn ensure_ok(resp: RpcResponse) -> Result<RpcResponse> {
    if resp.success {
        Ok(resp)
    } else {
        Err(SdkError::RequestFailed(
            resp.error.unwrap_or_else(|| "unknown error".into()),
        ))
    }
}

/// Best-effort default path for the `clu-rpc` binary.
///
/// Looks for a sibling `clu-rpc` next to the current executable (handles the
/// `.exe` suffix on Windows); otherwise returns the bare name and relies on
/// `PATH` resolution.
fn default_rpc_path() -> PathBuf {
    #[cfg(windows)]
    const NAME: &str = "clu-rpc.exe";
    #[cfg(not(windows))]
    const NAME: &str = "clu-rpc";

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(NAME);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(NAME)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use clu_core::protocol::AssistantMessageEvent;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Find the built `clu-rpc` binary in the workspace target directory.
    /// Returns `None` if it hasn't been built yet, so the test is skipped
    /// gracefully instead of failing.
    fn rpc_binary() -> Option<PathBuf> {
        // crates/sdk -> ../../target/debug/clu-rpc[.exe]
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace = PathBuf::from(manifest_dir).parent()?.parent()?.to_path_buf();
        #[cfg(windows)]
        let name = "clu-rpc.exe";
        #[cfg(not(windows))]
        let name = "clu-rpc";

        let candidate = workspace.join("target").join("debug").join(name);
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    }

    #[tokio::test]
    async fn prompt_roundtrip_via_real_rpc() {
        let Some(bin) = rpc_binary() else {
            eprintln!("skipping: clu-rpc binary not built");
            return;
        };

        let client = CluClient::spawn_with_path(&bin).await.expect("spawn");

        // Use the higher-level stream API to drain the full run.
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            client.run_prompt("sdk integration test"),
        )
        .await
        .expect("timeout sending prompt")
        .expect("prompt failed");

        let mut saw_agent_start = false;
        let mut saw_text_delta = false;
        let mut saw_agent_end = false;

        while let Some(evt) = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("timeout awaiting event")
        {
            match evt {
                Event::AgentStart => saw_agent_start = true,
                Event::MessageUpdate {
                    assistant_message_event,
                    ..
                } => {
                    if matches!(
                        assistant_message_event,
                        AssistantMessageEvent::TextDelta { .. }
                    ) {
                        saw_text_delta = true;
                    }
                }
                Event::AgentEnd { .. } => {
                    saw_agent_end = true;
                    break;
                }
                _ => {}
            }
        }

        assert!(saw_agent_start, "expected agent_start");
        assert!(saw_text_delta, "expected at least one text_delta");
        assert!(saw_agent_end, "expected agent_end");
    }

    #[tokio::test]
    async fn get_messages_returns_empty_stub_via_real_rpc() {
        let Some(bin) = rpc_binary() else {
            eprintln!("skipping: clu-rpc binary not built");
            return;
        };

        let client = CluClient::spawn_with_path(&bin).await.expect("spawn");
        let msgs = tokio::time::timeout(Duration::from_secs(5), client.get_messages())
            .await
            .expect("timeout")
            .expect("get_messages failed");
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn set_model_ok_via_real_rpc() {
        let Some(bin) = rpc_binary() else {
            eprintln!("skipping: clu-rpc binary not built");
            return;
        };

        let client = CluClient::spawn_with_path(&bin).await.expect("spawn");
        // Current stub returns no data; should still resolve Ok.
        let out = tokio::time::timeout(
            Duration::from_secs(5),
            client.set_model("openai", "gpt-4o"),
        )
        .await
        .expect("timeout")
        .expect("set_model failed");
        assert!(out.is_none());
    }
}
