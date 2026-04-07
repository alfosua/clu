use crate::{Message, Provider, ProviderEvent, ToolCall};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::error::Error;
use tokio::sync::mpsc;

/// An OpenAI-compatible chat-completions client. Works with OpenAI, Ollama
/// (`http://localhost:11434/v1`), LM Studio, vLLM, llama.cpp server, etc.
pub struct OpenAICompatible {
    client: Client,
    /// Base URL with no trailing slash, e.g. `http://localhost:11434/v1`.
    base_url: String,
    api_key: String,
}

impl OpenAICompatible {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let mut base_url = base_url.into();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        Self {
            client: Client::new(),
            base_url,
            api_key: api_key.into(),
        }
    }

    /// Build from environment variables, falling back to a local Ollama at
    /// `http://localhost:11434/v1`. Recognized vars (in priority order):
    ///   - `CLU_BASE_URL`, `OPENAI_BASE_URL`, `OPENAI_API_BASE`
    ///   - `CLU_API_KEY`,  `OPENAI_API_KEY`
    pub fn from_env() -> Self {
        let base_url = std::env::var("CLU_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .or_else(|_| std::env::var("OPENAI_API_BASE"))
            .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
        let api_key = std::env::var("CLU_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_else(|_| "ollama".to_string());
        Self::new(base_url, api_key)
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

fn message_to_json(m: &Message) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("role".into(), Value::String(m.role.clone()));
    obj.insert("content".into(), Value::String(m.content.clone()));
    if !m.tool_calls.is_empty() {
        let tcs: Vec<Value> = m
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }
                })
            })
            .collect();
        obj.insert("tool_calls".into(), Value::Array(tcs));
    }
    if let Some(id) = &m.tool_call_id {
        obj.insert("tool_call_id".into(), Value::String(id.clone()));
    }
    Value::Object(obj)
}

#[derive(Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[async_trait]
impl Provider for OpenAICompatible {
    async fn stream(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Value],
    ) -> Result<mpsc::Receiver<ProviderEvent>, Box<dyn Error + Send + Sync>> {
        let (tx, rx) = mpsc::channel(100);

        let msgs: Vec<Value> = messages.iter().map(message_to_json).collect();

        let mut body = json!({
            "model": model,
            "messages": msgs,
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.to_vec());
            body["tool_choice"] = Value::String("auto".into());
        }

        let response = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let _ = tx
                .send(ProviderEvent::Error(format!("HTTP {status}: {text}")))
                .await;
            return Ok(rx);
        }

        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            // SSE parse loop with cross-chunk line buffering.
            let mut buf = String::new();
            // tool-call accumulators keyed by `index`
            let mut partials: BTreeMap<u64, PartialToolCall> = BTreeMap::new();
            let mut done = false;

            'outer: while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(ProviderEvent::Error(e.to_string())).await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete lines; keep the trailing partial line in `buf`.
                loop {
                    let Some(nl) = buf.find('\n') else { break };
                    let line = buf[..nl].trim_end_matches('\r').to_string();
                    buf.drain(..=nl);

                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim_start();
                    if data.is_empty() {
                        continue;
                    }
                    if data == "[DONE]" {
                        done = true;
                        break 'outer;
                    }

                    let parsed: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let Some(choice) = parsed
                        .get("choices")
                        .and_then(|c| c.as_array())
                        .and_then(|a| a.first())
                    else {
                        continue;
                    };

                    if let Some(delta) = choice.get("delta") {
                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                let _ = tx
                                    .send(ProviderEvent::ContentDelta(content.to_string()))
                                    .await;
                            }
                        }
                        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                            for tc in tcs {
                                let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                                let entry = partials.entry(idx).or_default();
                                if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                    entry.id = Some(id.to_string());
                                }
                                if let Some(func) = tc.get("function") {
                                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                        // Names usually arrive in one piece, but some
                                        // backends chunk them — concatenate to be safe.
                                        match &mut entry.name {
                                            Some(n) => n.push_str(name),
                                            None => entry.name = Some(name.to_string()),
                                        }
                                    }
                                    if let Some(args) =
                                        func.get("arguments").and_then(|a| a.as_str())
                                    {
                                        entry.arguments.push_str(args);
                                    }
                                }
                            }
                        }
                    }

                    // Some providers (Ollama) signal completion via finish_reason
                    // without sending a `[DONE]` sentinel.
                    if choice
                        .get("finish_reason")
                        .map(|v| !v.is_null())
                        .unwrap_or(false)
                    {
                        done = true;
                    }
                }
            }

            for (_, p) in partials.into_iter() {
                if let (Some(id), Some(name)) = (p.id, p.name) {
                    let _ = tx
                        .send(ProviderEvent::ToolCall(ToolCall {
                            id,
                            name,
                            arguments: if p.arguments.is_empty() {
                                "{}".to_string()
                            } else {
                                p.arguments
                            },
                        }))
                        .await;
                }
            }

            let _ = done; // suppress unused warning when stream ends naturally
            let _ = tx.send(ProviderEvent::Done).await;
        });

        Ok(rx)
    }
}
