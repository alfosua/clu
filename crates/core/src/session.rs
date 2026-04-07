use crate::protocol::{
    AgentMessage, AssistantContentBlock, AssistantMessage, AssistantMessageEvent, Command, Event,
    StopReason, ToolResultMessage, Usage, UserContent, UserContentPart, UserMessage,
};
use crate::tools::Tool;
use clu_ai::{Message as AiMessage, Provider, ProviderEvent, ToolCall as AiToolCall};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

pub struct AgentSession {
    tools: HashMap<String, Arc<dyn Tool>>,
    model_name: String,
    provider_name: String,
    api: String,
    provider: Arc<dyn Provider>,
    history: Vec<AiMessage>,
}

impl AgentSession {
    pub fn new(provider: Arc<dyn Provider>, model_name: impl Into<String>) -> Self {
        let model_name = model_name.into();
        let history = vec![AiMessage::system(
            "You are clu, a concise terminal coding assistant. \
             When tools are available and useful, call them. Keep replies short.",
        )];
        Self {
            tools: HashMap::new(),
            model_name,
            provider_name: "openai-compatible".into(),
            api: "openai-completions".into(),
            provider,
            history,
        }
    }

    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        if let Some(first) = self.history.first_mut() {
            if first.role == "system" {
                first.content = prompt.into();
                return;
            }
        }
        self.history.insert(0, AiMessage::system(prompt));
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn new_partial(&self) -> AssistantMessage {
        AssistantMessage {
            content: vec![],
            api: self.api.clone(),
            provider: self.provider_name.clone(),
            model: self.model_name.clone(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: Self::now_ms(),
        }
    }

    fn tool_schemas(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.parameters(),
                    }
                })
            })
            .collect()
    }

    /// Run the agent session loop: receive Commands, emit Events.
    pub async fn run(
        &mut self,
        mut rx_cmd: mpsc::Receiver<Command>,
        tx_evt: mpsc::Sender<Event>,
    ) {
        while let Some(cmd) = rx_cmd.recv().await {
            match cmd {
                Command::Prompt { message, .. }
                | Command::FollowUp { message, .. }
                | Command::Steer { message, .. } => {
                    self.handle_prompt(message, &tx_evt).await;
                }
                Command::SetModel { provider, model_id } => {
                    self.provider_name = provider;
                    self.model_name = model_id;
                }
                Command::NewSession { .. } => {
                    let sys = self.history.first().cloned();
                    self.history.clear();
                    if let Some(s) = sys {
                        self.history.push(s);
                    }
                }
                Command::Abort => {}
                _ => {}
            }
        }
    }

    async fn handle_prompt(&mut self, message: String, tx_evt: &mpsc::Sender<Event>) {
        let _ = tx_evt.send(Event::AgentStart).await;

        self.history.push(AiMessage::user(message.clone()));
        let user_msg = AgentMessage::User(UserMessage {
            content: UserContent::Text(message),
            timestamp: Self::now_ms(),
        });
        let _ = tx_evt.send(Event::MessageStart { message: user_msg.clone() }).await;
        let _ = tx_evt.send(Event::MessageEnd { message: user_msg }).await;

        let max_iters: usize = std::env::var("CLU_MAX_ITERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(64);
        let mut emitted_messages: Vec<AgentMessage> = vec![];

        for _ in 0..max_iters {
            let _ = tx_evt.send(Event::TurnStart).await;

            let tools = self.tool_schemas();
            let stream_res = self
                .provider
                .stream(&self.model_name, &self.history, &tools)
                .await;

            let mut rx = match stream_res {
                Ok(rx) => rx,
                Err(e) => {
                    let mut partial = self.new_partial();
                    partial.stop_reason = StopReason::Error;
                    partial.error_message = Some(e.to_string());
                    partial.content = vec![AssistantContentBlock::Text {
                        text: format!("[provider error] {e}"),
                    }];
                    let assistant_msg = AgentMessage::Assistant(partial.clone());
                    let _ = tx_evt
                        .send(Event::MessageStart { message: assistant_msg.clone() })
                        .await;
                    let _ = tx_evt
                        .send(Event::MessageEnd { message: assistant_msg.clone() })
                        .await;
                    let _ = tx_evt
                        .send(Event::TurnEnd {
                            message: assistant_msg.clone(),
                            tool_results: vec![],
                        })
                        .await;
                    emitted_messages.push(assistant_msg);
                    break;
                }
            };

            let mut partial = self.new_partial();
            let assistant_msg_initial = AgentMessage::Assistant(partial.clone());
            let _ = tx_evt
                .send(Event::MessageStart { message: assistant_msg_initial.clone() })
                .await;
            let _ = tx_evt
                .send(Event::MessageUpdate {
                    message: assistant_msg_initial.clone(),
                    assistant_message_event: AssistantMessageEvent::Start {
                        partial: partial.clone(),
                    },
                })
                .await;
            let _ = tx_evt
                .send(Event::MessageUpdate {
                    message: assistant_msg_initial,
                    assistant_message_event: AssistantMessageEvent::TextStart {
                        content_index: 0,
                        partial: partial.clone(),
                    },
                })
                .await;

            let mut accumulated = String::new();
            let mut tool_calls: Vec<AiToolCall> = vec![];
            let mut errored: Option<String> = None;

            while let Some(evt) = rx.recv().await {
                match evt {
                    ProviderEvent::ContentDelta(d) => {
                        accumulated.push_str(&d);
                        partial.content = vec![AssistantContentBlock::Text {
                            text: accumulated.clone(),
                        }];
                        let _ = tx_evt
                            .send(Event::MessageUpdate {
                                message: AgentMessage::Assistant(partial.clone()),
                                assistant_message_event: AssistantMessageEvent::TextDelta {
                                    content_index: 0,
                                    delta: d,
                                    partial: partial.clone(),
                                },
                            })
                            .await;
                    }
                    ProviderEvent::ToolCall(tc) => tool_calls.push(tc),
                    ProviderEvent::Error(e) => {
                        errored = Some(e);
                        break;
                    }
                    ProviderEvent::Done => break,
                }
            }

            let mut content_blocks: Vec<AssistantContentBlock> = vec![];
            if !accumulated.is_empty() {
                content_blocks.push(AssistantContentBlock::Text {
                    text: accumulated.clone(),
                });
                let _ = tx_evt
                    .send(Event::MessageUpdate {
                        message: AgentMessage::Assistant(partial.clone()),
                        assistant_message_event: AssistantMessageEvent::TextEnd {
                            content_index: 0,
                            content: accumulated.clone(),
                            partial: partial.clone(),
                        },
                    })
                    .await;
            }
            for tc in &tool_calls {
                let args_val: Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or(Value::String(tc.arguments.clone()));
                content_blocks.push(AssistantContentBlock::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: args_val,
                });
            }

            partial.content = content_blocks;
            partial.stop_reason = if errored.is_some() {
                StopReason::Error
            } else if !tool_calls.is_empty() {
                StopReason::ToolUse
            } else {
                StopReason::Stop
            };
            partial.error_message = errored.clone();

            let assistant_msg = AgentMessage::Assistant(partial.clone());
            let _ = tx_evt
                .send(Event::MessageUpdate {
                    message: assistant_msg.clone(),
                    assistant_message_event: AssistantMessageEvent::Done {
                        reason: partial.stop_reason.clone(),
                        message: partial.clone(),
                    },
                })
                .await;
            let _ = tx_evt
                .send(Event::MessageEnd { message: assistant_msg.clone() })
                .await;

            self.history.push(AiMessage::assistant_with_tools(
                accumulated.clone(),
                tool_calls.clone(),
            ));

            let mut tool_results: Vec<ToolResultMessage> = vec![];
            for tc in &tool_calls {
                let args_val: Value =
                    serde_json::from_str(&tc.arguments).unwrap_or(Value::Null);
                let _ = tx_evt
                    .send(Event::ToolExecutionStart {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        args: args_val.clone(),
                    })
                    .await;

                let (result_str, is_error) = match self.tools.get(&tc.name) {
                    Some(tool) => match tool.execute(args_val.clone()).await {
                        Ok(s) => (s, false),
                        Err(e) => (format!("error: {e}"), true),
                    },
                    None => (format!("error: unknown tool '{}'", tc.name), true),
                };

                let _ = tx_evt
                    .send(Event::ToolExecutionEnd {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        result: Value::String(result_str.clone()),
                        is_error,
                    })
                    .await;

                self.history
                    .push(AiMessage::tool(tc.id.clone(), result_str.clone()));

                tool_results.push(ToolResultMessage {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    content: vec![UserContentPart::Text { text: result_str }],
                    is_error,
                    timestamp: Self::now_ms(),
                });
            }

            let _ = tx_evt
                .send(Event::TurnEnd {
                    message: assistant_msg.clone(),
                    tool_results,
                })
                .await;
            emitted_messages.push(assistant_msg);

            if tool_calls.is_empty() || errored.is_some() {
                break;
            }
        }

        let _ = tx_evt
            .send(Event::AgentEnd {
                messages: emitted_messages,
            })
            .await;
    }
}
