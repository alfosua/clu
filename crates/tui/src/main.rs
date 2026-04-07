//! clu-tui — minimal but usable Ratatui frontend that drives a `clu-rpc`
//! subprocess over JSONL stdin/stdout.

use clu_core::protocol::{AssistantMessageEvent, Command, Event};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command as TokioCommand};
use tokio::sync::mpsc;

#[derive(Clone)]
enum Line2 {
    User(String),
    Assistant(String),
    Tool(String),
    System(String),
    Error(String),
}

struct App {
    lines: Vec<Line2>,
    /// Currently streaming assistant text. Pushed into `lines` on Done.
    streaming: String,
    input: String,
    busy: bool,
    scroll: u16,
    model: String,
}

impl App {
    fn new(model: String) -> Self {
        Self {
            lines: vec![Line2::System(
                "clu — type a message and press Enter. Ctrl+C or Esc to quit.".into(),
            )],
            streaming: String::new(),
            input: String::new(),
            busy: false,
            scroll: 0,
            model,
        }
    }

    fn render_lines(&self) -> Vec<Line<'_>> {
        let mut out: Vec<Line> = Vec::new();
        for l in &self.lines {
            match l {
                Line2::User(s) => out.extend(prefixed("you", Color::Cyan, s)),
                Line2::Assistant(s) => out.extend(prefixed("clu", Color::Green, s)),
                Line2::Tool(s) => out.extend(prefixed("tool", Color::Yellow, s)),
                Line2::System(s) => out.push(Line::from(Span::styled(
                    s.clone(),
                    Style::default().fg(Color::DarkGray),
                ))),
                Line2::Error(s) => out.push(Line::from(Span::styled(
                    format!("error: {s}"),
                    Style::default().fg(Color::Red),
                ))),
            }
            out.push(Line::from(""));
        }
        if !self.streaming.is_empty() {
            out.extend(prefixed("clu", Color::Green, &self.streaming));
        }
        out
    }
}

fn prefixed<'a>(label: &'a str, color: Color, body: &'a str) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut first = true;
    for raw in body.split('\n') {
        if first {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{label}: "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(raw.to_string()),
            ]));
            first = false;
        } else {
            lines.push(Line::from(Span::raw(raw.to_string())));
        }
    }
    lines
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = std::env::var("CLU_MODEL").unwrap_or_else(|_| "llama3.2".to_string());

    // Spawn clu-rpc as a sibling binary; fall back to `cargo run` for dev.
    let mut rpc_child = spawn_rpc()?;
    let mut rpc_stdin = rpc_child.stdin.take().expect("rpc stdin");
    let rpc_stdout = rpc_child.stdout.take().expect("rpc stdout");

    let (tx_evt, mut rx_evt) = mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        let mut reader = BufReader::new(rpc_stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(evt) = serde_json::from_str::<Event>(&line) {
                if tx_evt.send(evt).is_err() {
                    break;
                }
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(model.clone());
    let res = run_app(&mut terminal, &mut app, &mut rpc_stdin, &mut rx_evt).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let _ = rpc_child.kill().await;
    res
}

fn spawn_rpc() -> io::Result<tokio::process::Child> {
    if let Some(path) = sibling_clu_rpc() {
        return TokioCommand::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
    }
    TokioCommand::new("cargo")
        .args(["run", "--quiet", "--bin", "clu-rpc"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

fn sibling_clu_rpc() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) { "clu-rpc.exe" } else { "clu-rpc" };
    let p = dir.join(name);
    if p.exists() { Some(p) } else { None }
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rpc_stdin: &mut ChildStdin,
    rx_evt: &mut mpsc::UnboundedReceiver<Event>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        // Wake on either an RPC event or a short tick (for key polling).
        tokio::select! {
            biased;
            maybe_evt = rx_evt.recv() => {
                if let Some(evt) = maybe_evt {
                    handle_event(app, evt);
                }
                // Drain anything else queued so we redraw once for the burst.
                while let Ok(more) = rx_evt.try_recv() {
                    handle_event(app, more);
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(16)) => {}
        }

        // Non-blocking key drain.
        let mut should_break = false;
        while event::poll(std::time::Duration::from_millis(0))? {
            if let CEvent::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => { should_break = true; break; }
                    (KeyCode::Enter, _) => {
                        let msg = app.input.trim().to_string();
                        if msg.is_empty() || app.busy {
                            continue;
                        }
                        app.input.clear();
                        app.lines.push(Line2::User(msg.clone()));
                        app.busy = true;
                        let cmd = Command::Prompt {
                            message: msg,
                            images: None,
                            streaming_behavior: None,
                        };
                        let mut line = serde_json::to_string(&cmd).unwrap();
                        line.push('\n');
                        if rpc_stdin.write_all(line.as_bytes()).await.is_err() {
                            app.lines.push(Line2::Error("rpc pipe closed".into()));
                            app.busy = false;
                        } else {
                            let _ = rpc_stdin.flush().await;
                        }
                    }
                    (KeyCode::Backspace, _) => {
                        app.input.pop();
                    }
                    (KeyCode::Char(c), _) => app.input.push(c),
                    (KeyCode::PageUp, _) => app.scroll = app.scroll.saturating_add(5),
                    (KeyCode::PageDown, _) => app.scroll = app.scroll.saturating_sub(5),
                    _ => {}
                }
            }
        }
        if should_break { break; }
    }
    Ok(())
}

fn handle_event(app: &mut App, evt: Event) {
    match evt {
        Event::AgentStart => {
            app.busy = true;
            app.streaming.clear();
        }
        Event::AgentEnd { .. } => {
            app.busy = false;
        }
        Event::MessageUpdate { assistant_message_event, .. } => match assistant_message_event {
            AssistantMessageEvent::TextDelta { delta, .. } => {
                app.streaming.push_str(&delta);
            }
            AssistantMessageEvent::TextEnd { content, .. } => {
                if !content.is_empty() {
                    app.lines.push(Line2::Assistant(content));
                }
                app.streaming.clear();
            }
            AssistantMessageEvent::Error { error, .. } => {
                app.lines.push(Line2::Error(
                    error.error_message.unwrap_or_else(|| "unknown".into()),
                ));
                app.streaming.clear();
            }
            _ => {}
        },
        Event::ToolExecutionStart { tool_name, args, .. } => {
            app.lines
                .push(Line2::Tool(format!("→ {tool_name}({args})")));
        }
        Event::ToolExecutionEnd { tool_name, result, is_error, .. } => {
            let text = result.as_str().map(str::to_string).unwrap_or_else(|| result.to_string());
            let preview: String = text.lines().take(20).collect::<Vec<_>>().join("\n");
            let prefix = if is_error { "✗" } else { "←" };
            app.lines
                .push(Line2::Tool(format!("{prefix} {tool_name}\n{preview}")));
        }
        _ => {}
    }
}

fn draw(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // history
            Constraint::Length(3), // input
            Constraint::Length(1), // status
        ])
        .split(f.size());

    let lines = app.render_lines();
    let total: u16 = lines.len().try_into().unwrap_or(u16::MAX);
    let viewport_h = chunks[0].height.saturating_sub(2); // borders
    let max_scroll = total.saturating_sub(viewport_h);
    let auto = max_scroll.saturating_sub(app.scroll);

    let history = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" clu · {} ", app.model))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false })
        .scroll((auto, 0));
    f.render_widget(history, chunks[0]);

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .block(Block::default().title(" message ").borders(Borders::ALL));
    f.render_widget(input, chunks[1]);

    let status_text = if app.busy {
        "streaming…  Esc/Ctrl+C quit  PgUp/PgDn scroll"
    } else {
        "ready.  Enter send  Esc/Ctrl+C quit  PgUp/PgDn scroll"
    };
    let status = Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[2]);

    // Cursor inside input box
    let cx = chunks[1].x + 1 + app.input.chars().count() as u16;
    let cy = chunks[1].y + 1;
    f.set_cursor(cx.min(chunks[1].x + chunks[1].width.saturating_sub(2)), cy);
}
