//! Clawd inference mesh client (OpenAI-compatible `/v1/chat/completions`).
//!
//! rig-core's OpenAI client builds URLs with `.replace("//", "/")`, which
//! corrupts `https://` → `https:/` and can yield empty/broken requests against
//! our mesh. This module posts to a correct URL.

use anyhow::Result;
use rig::agent::AgentBuilder;
use rig::completion::{
    CompletionError, CompletionModel as CompletionModelTrait, CompletionRequest,
    CompletionResponse as RigCompletionResponse,
};
use rig::message::{AssistantContent, Message as RigMessage, Text, ToolCall, ToolFunction};
use rig::OneOrMany;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::common::{mesh_api_key, mesh_base_url, mesh_model};

#[derive(Clone)]
pub struct MeshClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl MeshClient {
    pub fn from_env() -> Self {
        let base = mesh_base_url().trim_end_matches('/').to_string();
        Self {
            base_url: base,
            api_key: mesh_api_key(),
            http: reqwest::Client::new(),
        }
    }

    pub fn agent(&self, model: &str) -> AgentBuilder<MeshCompletionModel> {
        AgentBuilder::new(MeshCompletionModel {
            client: Arc::new(self.clone()),
            model: model.to_string(),
        })
    }

    fn chat_url(&self) -> String {
        // base is .../v1 → .../v1/chat/completions (no // mangling)
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

#[derive(Clone)]
pub struct MeshCompletionModel {
    client: Arc<MeshClient>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseToolFunction,
}

#[derive(Debug, Deserialize)]
struct ResponseToolFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OutMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

fn rig_message_to_openai(msg: &RigMessage) -> Vec<OutMessage> {
    match msg {
        RigMessage::User { content } => {
            let mut texts = Vec::new();
            let mut tool_results = Vec::new();
            for c in content.iter() {
                match c {
                    rig::message::UserContent::Text(t) => texts.push(t.text.clone()),
                    rig::message::UserContent::ToolResult(tr) => {
                        let body = tr
                            .content
                            .iter()
                            .filter_map(|x| match x {
                                rig::message::ToolResultContent::Text(t) => Some(t.text.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        tool_results.push(OutMessage {
                            role: "tool".into(),
                            content: Some(body),
                            tool_calls: None,
                            tool_call_id: Some(tr.id.clone()),
                        });
                    }
                    _ => {}
                }
            }
            let mut out = Vec::new();
            if !texts.is_empty() {
                out.push(OutMessage {
                    role: "user".into(),
                    content: Some(texts.join("\n")),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            out.extend(tool_results);
            out
        }
        RigMessage::Assistant { content } => {
            let mut texts = Vec::new();
            let mut tool_calls = Vec::new();
            for c in content.iter() {
                match c {
                    AssistantContent::Text(t) => texts.push(t.text.clone()),
                    AssistantContent::ToolCall(tc) => {
                        tool_calls.push(json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments.to_string(),
                            }
                        }));
                    }
                }
            }
            vec![OutMessage {
                role: "assistant".into(),
                content: if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id: None,
            }]
        }
    }
}

impl CompletionModelTrait for MeshCompletionModel {
    type Response = Value;

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> std::result::Result<RigCompletionResponse<Value>, CompletionError> {
        let mut messages: Vec<OutMessage> = Vec::new();
        if let Some(preamble) = &request.preamble {
            if !preamble.trim().is_empty() {
                messages.push(OutMessage {
                    role: "system".into(),
                    content: Some(preamble.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }
        for m in &request.chat_history {
            messages.extend(rig_message_to_openai(m));
        }
        messages.extend(rig_message_to_openai(&request.prompt));

        if messages.is_empty() {
            return Err(CompletionError::RequestError(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "mesh: no messages to send")
                    .into(),
            ));
        }

        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });
        if let Some(max) = request.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }

        let url = self.client.chat_url();
        tracing::info!(%url, model = %self.model, n_messages = messages.len(), "mesh completion");

        let resp = self
            .client
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.client.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| CompletionError::RequestError(e.into()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| CompletionError::RequestError(e.into()))?;

        if !status.is_success() {
            return Err(CompletionError::ProviderError(text));
        }

        // Some free routes return SSE even when stream:false — take first data line if needed
        let json_text = if text.trim_start().starts_with("data:") {
            text.lines()
                .find_map(|l| l.strip_prefix("data: "))
                .filter(|l| *l != "[DONE]")
                .unwrap_or(&text)
                .to_string()
        } else {
            text
        };

        let parsed: ChatCompletionResponse = serde_json::from_str(&json_text)
            .map_err(|e| CompletionError::ResponseError(format!("{e}: {json_text}")))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| CompletionError::ResponseError("no choices".into()))?;

        let mut contents: Vec<AssistantContent> = Vec::new();
        if let Some(c) = choice.message.content {
            if !c.is_empty() {
                contents.push(AssistantContent::Text(Text { text: c }));
            }
        }
        if let Some(tcs) = choice.message.tool_calls {
            for tc in tcs {
                let args: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or_else(|_| json!({ "raw": tc.function.arguments }));
                contents.push(AssistantContent::ToolCall(ToolCall {
                    id: tc.id,
                    function: ToolFunction {
                        name: tc.function.name,
                        arguments: args,
                    },
                }));
            }
        }
        if contents.is_empty() {
            contents.push(AssistantContent::Text(Text {
                text: String::new(),
            }));
        }

        Ok(RigCompletionResponse {
            choice: OneOrMany::many(contents)
                .unwrap_or_else(|_| OneOrMany::one(AssistantContent::Text(Text { text: "".into() }))),
            raw_response: json!({}),
        })
    }
}

/// Default agent builder on the Clawd mesh.
pub fn mesh_agent_builder() -> AgentBuilder<MeshCompletionModel> {
    let client = MeshClient::from_env();
    let model = mesh_model();
    tracing::info!(
        base = %client.base_url,
        %model,
        "using Clawd inference mesh"
    );
    client.agent(&model)
}

/// Quick connectivity check used by tests / doctor.
pub async fn mesh_health_ping() -> Result<String> {
    let base = mesh_base_url();
    // .../v1 → origin /health
    let origin = base
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string();
    let url = format!("{origin}/health");
    let text = reqwest::get(&url).await?.text().await?;
    Ok(text)
}
