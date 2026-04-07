//! `clu` — the single entry-point binary for the Core LLM Utility.
//!
//! See `SPECS.md` in this crate for the user-facing CLI specification.
//!
//! Dispatching strategy:
//! - `--rpc`        → exec the sibling `clu-rpc` binary (same protocol; transparent pass-through).
//! - `--quick`/`-q` → spawn `clu-rpc` as a subprocess and drive it non-interactively.
//! - *(default)*    → exec the sibling `clu-tui` binary.
//!
//! Sibling binaries are resolved next to the `clu` executable itself, so an
//! installed layout like `~/.cargo/bin/{clu,clu-rpc,clu-tui}` just works.

use clap::Parser;
use clu_core::protocol::{AssistantMessageEvent, Command, Event};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command as TokioCommand;

// ============================================================================
// CLI definition
// ============================================================================

/// clu — Core LLM Utility: a minimal AI agent with a terminal bridge architecture.
#[derive(Debug, Parser)]
#[command(
    name = "clu",
    version,
    about = "Core LLM Utility — minimal AI agent with terminal bridge architecture",
    long_about = None,
    disable_help_subcommand = true,
)]
struct Cli {
    // ── Mode flags ─────────────────────────────────────────────────────────
    /// Run as an RPC server using stdin/stdout (JSONL framing).
    #[arg(long, conflicts_with_all = ["quick", "continue_", "resume"])]
    rpc: bool,

    /// Non-interactive mode: send the prompt, stream output, exit.
    /// Additional positional MESSAGEs are queued as follow-ups.
    #[arg(short = 'q', long, value_name = "PROMPT")]
    quick: Option<String>,

    // ── Session flags ──────────────────────────────────────────────────────
    /// Continue the most recent session in the current working directory.
    #[arg(short = 'c', long = "continue", conflicts_with = "resume")]
    continue_: bool,

    /// Open the session picker to resume an existing session.
    #[arg(short = 'r', long)]
    resume: bool,

    /// Load a specific session file (.jsonl). Overrides --continue / --resume.
    #[arg(long, value_name = "PATH")]
    session: Option<PathBuf>,

    /// Use a custom directory for session storage.
    #[arg(long, value_name = "PATH")]
    storage: Option<PathBuf>,

    // ── Model flags ────────────────────────────────────────────────────────
    /// Preferred provider (e.g. `openai`, `anthropic`, `openai-compatible`).
    #[arg(long, value_name = "NAME")]
    provider: Option<String>,

    /// Active model. Accepts `id`, `provider/id`, `id:thinking`, or a glob.
    #[arg(long, value_name = "PATTERN")]
    model: Option<String>,

    /// Comma-separated set of models available for cycling.
    #[arg(long, value_name = "LIST", value_delimiter = ',')]
    models: Vec<String>,

    /// Comma-separated set of providers the client is allowed to use.
    #[arg(long, value_name = "LIST", value_delimiter = ',')]
    providers: Vec<String>,

    /// Reasoning level.
    #[arg(long, value_name = "LEVEL",
          value_parser = ["off", "minimal", "low", "medium", "high", "xhigh"])]
    thinking: Option<String>,

    // ── Tool flags ─────────────────────────────────────────────────────────
    /// Comma-separated set of built-in tools to enable
    /// (read, grep, find, ls, write, edit, bash).
    #[arg(long, value_name = "LIST", value_delimiter = ',')]
    tools: Vec<String>,

    // ── Positional messages ────────────────────────────────────────────────
    /// Initial prompt(s). Multiple messages are queued as follow-ups.
    #[arg(value_name = "MESSAGE")]
    messages: Vec<String>,
}

/// The dispatch target after argument validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Tui,
    Rpc,
    Quick,
}

impl Cli {
    fn mode(&self) -> Mode {
        if self.rpc {
            Mode::Rpc
        } else if self.quick.is_some() {
            Mode::Quick
        } else {
            Mode::Tui
        }
    }

    /// All prompts for quick mode, in delivery order. `--quick <PROMPT>` comes
    /// first, followed by any positional `MESSAGE`s.
    fn quick_prompts(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(p) = &self.quick {
            out.push(p.clone());
        }
        out.extend(self.messages.iter().cloned());
        out
    }
}

// ============================================================================
// Entry point
// ============================================================================

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.mode() {
        Mode::Rpc => run_rpc(&cli).await,
        Mode::Quick => run_quick(&cli).await,
        Mode::Tui => run_tui(&cli).await,
    };

    std::process::exit(exit_code);
}

// ============================================================================
// Mode: RPC  (transparent exec of clu-rpc)
// ============================================================================

async fn run_rpc(cli: &Cli) -> i32 {
    let bin = match sibling_binary("clu-rpc") {
        Some(p) => p,
        None => {
            eprintln!("clu: could not locate `clu-rpc` next to the `clu` binary");
            return 1;
        }
    };

    let mut tc = TokioCommand::new(bin);
    if !cli.tools.is_empty() {
        tc.env("CLU_TOOLS", cli.tools.join(","));
    }
    let status = tc
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await;

    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("clu: failed to spawn clu-rpc: {e}");
            1
        }
    }
}

// ============================================================================
// Mode: TUI  (transparent exec of clu-tui)
// ============================================================================

async fn run_tui(cli: &Cli) -> i32 {
    // TODO: forward session / model / tool flags and positional messages to clu-tui.
    if !cli.messages.is_empty() {
        eprintln!("clu: note — forwarding positional messages to the TUI is not yet wired up");
    }

    let bin = match sibling_binary("clu-tui") {
        Some(p) => p,
        None => {
            eprintln!("clu: could not locate `clu-tui` next to the `clu` binary");
            return 1;
        }
    };

    let mut tc = TokioCommand::new(bin);
    if !cli.tools.is_empty() {
        tc.env("CLU_TOOLS", cli.tools.join(","));
    }
    let status = tc
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await;

    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("clu: failed to spawn clu-tui: {e}");
            1
        }
    }
}

// ============================================================================
// Mode: Quick  (non-interactive; drive clu-rpc ourselves)
// ============================================================================

async fn run_quick(cli: &Cli) -> i32 {
    let prompts = cli.quick_prompts();
    if prompts.is_empty() {
        eprintln!("clu: --quick requires at least one prompt");
        return 2;
    }

    let bin = match sibling_binary("clu-rpc") {
        Some(p) => p,
        None => {
            eprintln!("clu: could not locate `clu-rpc` next to the `clu` binary");
            return 1;
        }
    };

    let mut tc = TokioCommand::new(bin);
    if !cli.tools.is_empty() {
        tc.env("CLU_TOOLS", cli.tools.join(","));
    }
    let mut child = match tc
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("clu: failed to spawn clu-rpc: {e}");
            return 1;
        }
    };

    let mut stdin = child.stdin.take().expect("clu-rpc stdin piped");
    let stdout = child.stdout.take().expect("clu-rpc stdout piped");
    let mut reader = BufReader::new(stdout).lines();

    // ── Apply startup options via RPC commands ─────────────────────────────
    if let Some(model) = &cli.model {
        let (provider, model_id) = split_model(cli.provider.as_deref(), model);
        if let Some(prov) = provider {
            let cmd = Command::SetModel {
                provider: prov,
                model_id,
            };
            let _ = write_cmd(&mut stdin, &cmd).await;
        }
    }

    if let Some(level) = &cli.thinking {
        if let Some(parsed) = parse_thinking(level) {
            let cmd = Command::SetThinkingLevel { level: parsed };
            let _ = write_cmd(&mut stdin, &cmd).await;
        }
    }

    // ── Send the first prompt, queue the rest as follow-ups ───────────────
    let mut iter = prompts.into_iter();
    let first = iter.next().unwrap();
    let cmd = Command::Prompt {
        message: first,
        images: None,
        streaming_behavior: None,
    };
    if let Err(e) = write_cmd(&mut stdin, &cmd).await {
        eprintln!("clu: failed to send prompt: {e}");
        return 1;
    }

    for msg in iter {
        let cmd = Command::FollowUp {
            message: msg,
            images: None,
        };
        if let Err(e) = write_cmd(&mut stdin, &cmd).await {
            eprintln!("clu: failed to queue follow-up: {e}");
            return 1;
        }
    }

    // Track outstanding "agent runs" = 1 (initial) + number of follow-ups.
    // We consider quick mode done when every run has ended.
    //
    // Simpler heuristic used here: exit on the FIRST `agent_end` that also
    // drains any queued follow-ups. The follow-up mode default in pi-style
    // protocols is "one-at-a-time", so multiple runs will happen; we track
    // them by counting `agent_start` vs `agent_end`.
    let mut runs_open: i64 = 0;
    let mut any_started = false;

    while let Ok(Some(line)) = reader.next_line().await {
        // Each line is either an RpcOutput::Response or an RpcOutput::Event.
        // For quick mode we only care about Events; responses are acks.
        let Ok(evt) = serde_json::from_str::<Event>(&line) else {
            continue;
        };

        match evt {
            Event::AgentStart => {
                runs_open += 1;
                any_started = true;
            }
            Event::AgentEnd { .. } => {
                runs_open -= 1;
                if any_started && runs_open <= 0 {
                    // All runs drained — we're done.
                    break;
                }
            }
            Event::MessageUpdate {
                assistant_message_event,
                ..
            } => {
                // Stream text deltas to stdout as they arrive.
                if let AssistantMessageEvent::TextDelta { delta, .. } = assistant_message_event {
                    print!("{delta}");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
            Event::TurnEnd { .. } => {
                // Newline between turns for readability.
                println!();
            }
            _ => {}
        }
    }

    // Close stdin so clu-rpc exits cleanly.
    drop(stdin);
    let _ = child.wait().await;
    0
}

// ============================================================================
// Helpers
// ============================================================================

/// Locate a sibling binary (e.g. `clu-rpc`, `clu-tui`) next to the running
/// `clu` executable. Falls back to `PATH` lookup via `which`-style search.
fn sibling_binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;

    let candidate = dir.join(exe_name(name));
    if candidate.exists() {
        return Some(candidate);
    }

    // Fallback: trust PATH.
    Some(PathBuf::from(exe_name(name)))
}

#[cfg(windows)]
fn exe_name(name: &str) -> String {
    format!("{name}.exe")
}

#[cfg(not(windows))]
fn exe_name(name: &str) -> String {
    name.to_string()
}

/// Split a model spec into `(provider, model_id)`.
///
/// Accepts: `id`, `provider/id`, `id:thinking`, `provider/id:thinking`.
/// When the spec has no `provider/` prefix and no explicit `--provider` is
/// given, returns `(None, id)`.
fn split_model(cli_provider: Option<&str>, spec: &str) -> (Option<String>, String) {
    let (provider, rest) = match spec.split_once('/') {
        Some((p, r)) => (Some(p.to_string()), r.to_string()),
        None => (cli_provider.map(str::to_string), spec.to_string()),
    };

    // Strip the optional :thinking suffix from the model id.
    let model_id = rest.split(':').next().unwrap_or(&rest).to_string();
    (provider, model_id)
}

fn parse_thinking(s: &str) -> Option<clu_core::protocol::ThinkingLevel> {
    use clu_core::protocol::ThinkingLevel;
    Some(match s {
        "off" => ThinkingLevel::Off,
        "minimal" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        "xhigh" => ThinkingLevel::Xhigh,
        _ => return None,
    })
}

async fn write_cmd<W>(w: &mut W, cmd: &Command) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut line = serde_json::to_string(cmd).unwrap();
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_model_bare_id() {
        let (p, m) = split_model(None, "gpt-4o");
        assert_eq!(p, None);
        assert_eq!(m, "gpt-4o");
    }

    #[test]
    fn split_model_bare_id_with_flag_provider() {
        let (p, m) = split_model(Some("openai"), "gpt-4o");
        assert_eq!(p.as_deref(), Some("openai"));
        assert_eq!(m, "gpt-4o");
    }

    #[test]
    fn split_model_inline_provider() {
        let (p, m) = split_model(None, "anthropic/claude-sonnet-4");
        assert_eq!(p.as_deref(), Some("anthropic"));
        assert_eq!(m, "claude-sonnet-4");
    }

    #[test]
    fn split_model_with_thinking_suffix() {
        let (p, m) = split_model(None, "anthropic/sonnet:high");
        assert_eq!(p.as_deref(), Some("anthropic"));
        assert_eq!(m, "sonnet");
    }

    #[test]
    fn parse_thinking_levels() {
        assert!(parse_thinking("off").is_some());
        assert!(parse_thinking("xhigh").is_some());
        assert!(parse_thinking("bogus").is_none());
    }

    #[test]
    fn cli_defaults_to_tui_mode() {
        let cli = Cli::parse_from(["clu"]);
        assert_eq!(cli.mode(), Mode::Tui);
    }

    #[test]
    fn cli_rpc_flag_selects_rpc_mode() {
        let cli = Cli::parse_from(["clu", "--rpc"]);
        assert_eq!(cli.mode(), Mode::Rpc);
    }

    #[test]
    fn cli_quick_flag_selects_quick_mode() {
        let cli = Cli::parse_from(["clu", "-q", "hi"]);
        assert_eq!(cli.mode(), Mode::Quick);
        assert_eq!(cli.quick_prompts(), vec!["hi".to_string()]);
    }

    #[test]
    fn cli_quick_with_positional_messages() {
        let cli = Cli::parse_from(["clu", "-q", "first", "second", "third"]);
        assert_eq!(cli.mode(), Mode::Quick);
        assert_eq!(
            cli.quick_prompts(),
            vec!["first".to_string(), "second".to_string(), "third".to_string()]
        );
    }

    #[test]
    fn cli_positional_messages_only_defaults_to_tui() {
        let cli = Cli::parse_from(["clu", "Fix this", "And that"]);
        assert_eq!(cli.mode(), Mode::Tui);
        assert_eq!(cli.messages.len(), 2);
    }

    #[test]
    fn cli_continue_and_resume_are_mutually_exclusive() {
        let err = Cli::try_parse_from(["clu", "-c", "-r"]);
        assert!(err.is_err());
    }

    #[test]
    fn cli_rpc_and_quick_are_mutually_exclusive() {
        let err = Cli::try_parse_from(["clu", "--rpc", "-q", "hi"]);
        assert!(err.is_err());
    }

    #[test]
    fn cli_models_list_comma_separated() {
        let cli = Cli::parse_from(["clu", "--models", "gpt-4o,claude-sonnet-4,gemini-pro"]);
        assert_eq!(cli.models, vec!["gpt-4o", "claude-sonnet-4", "gemini-pro"]);
    }

    #[test]
    fn cli_tools_list_comma_separated() {
        let cli = Cli::parse_from(["clu", "--tools", "read,grep,find,ls"]);
        assert_eq!(cli.tools, vec!["read", "grep", "find", "ls"]);
    }

    #[test]
    fn cli_thinking_validates_level() {
        assert!(Cli::try_parse_from(["clu", "--thinking", "high"]).is_ok());
        assert!(Cli::try_parse_from(["clu", "--thinking", "bogus"]).is_err());
    }

    #[test]
    fn cli_continue_flag_parses() {
        let cli = Cli::parse_from(["clu", "--continue"]);
        assert!(cli.continue_);
        let cli = Cli::parse_from(["clu", "-c"]);
        assert!(cli.continue_);
    }

    #[test]
    fn cli_resume_flag_parses() {
        let cli = Cli::parse_from(["clu", "--resume"]);
        assert!(cli.resume);
        let cli = Cli::parse_from(["clu", "-r"]);
        assert!(cli.resume);
    }
}
