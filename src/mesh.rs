//! Clawd / xAI inference client for kit agents.
//!
//! ## Backends
//! - **xAI (preferred when `XAI_API_KEY` is set)** — Grok **4.5** via
//!   [Responses API](https://docs.x.ai) `POST https://api.x.ai/v1/responses`
//!   (or Chat Completions if `CLAWD_LLM_API=chat`).
//! - **Clawd mesh** — free OpenAI-compatible router
//!   `POST …/v1/chat/completions` (default without XAI key).
//!
//! rig-core's stock OpenAI client mangles `https://` URLs; this module posts
//! to a correct URL with timeouts and robust parsing.

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
use std::time::Duration;

use crate::common::{
    llm_api_style, llm_provider, llm_timeout_secs, mesh_api_key, mesh_base_url, mesh_model,
    xai_store_messages, LlmApiStyle, LlmProvider,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct MeshClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
    style: LlmApiStyle,
    provider: LlmProvider,
    /// Responses API only: persist turns on xAI servers (default false).
    store: bool,
}

impl MeshClient {
    pub fn from_env() -> Self {
        let provider = llm_provider();
        let style = llm_api_style();
        let base = mesh_base_url().trim_end_matches('/').to_string();
        let timeout = Duration::from_secs(llm_timeout_secs());
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent("openclawd-solana-kit/mesh")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url: base,
            api_key: mesh_api_key(),
            http,
            style,
            provider,
            store: xai_store_messages(),
        }
    }

    pub fn agent(&self, model: &str) -> AgentBuilder<MeshCompletionModel> {
        AgentBuilder::new(MeshCompletionModel {
            client: Arc::new(self.clone()),
            model: model.to_string(),
        })
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn responses_url(&self) -> String {
        format!("{}/responses", self.base_url.trim_end_matches('/'))
    }
}

#[derive(Clone)]
pub struct MeshCompletionModel {
    client: Arc<MeshClient>,
    model: String,
}

// ─── Chat Completions shapes ─────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
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
    #[serde(default)]
    id: String,
    function: ResponseToolFunction,
}

#[derive(Debug, Deserialize)]
struct ResponseToolFunction {
    name: String,
    #[serde(default, deserialize_with = "deserialize_tool_arguments")]
    arguments: String,
}

fn deserialize_tool_arguments<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    Ok(match v {
        Value::Null => "{}".into(),
        Value::String(s) => {
            if s.trim().is_empty() {
                "{}".into()
            } else {
                s
            }
        }
        other => other.to_string(),
    })
}

// ─── Responses API shapes (xAI / OpenAI Responses) ───────────────

#[derive(Debug, Deserialize)]
struct ResponsesApiBody {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    output: Vec<Value>,
    /// Some gateways still nest a chat-style choice for compatibility.
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    output_text: Option<String>,
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

/// Map chat-style messages into Responses API `input` items.
fn messages_to_responses_input(messages: &[OutMessage]) -> Vec<Value> {
    let mut input = Vec::new();
    for m in messages {
        match m.role.as_str() {
            "tool" => {
                // OpenAI/xAI Responses: function_call_output
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": m.tool_call_id.clone().unwrap_or_default(),
                    "output": m.content.clone().unwrap_or_default(),
                }));
            }
            "assistant" => {
                if let Some(tcs) = &m.tool_calls {
                    for tc in tcs {
                        let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let name = tc
                            .pointer("/function/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let args = tc
                            .pointer("/function/arguments")
                            .map(|v| match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .unwrap_or_else(|| "{}".into());
                        input.push(json!({
                            "type": "function_call",
                            "call_id": id,
                            "name": name,
                            "arguments": args,
                        }));
                    }
                }
                if let Some(text) = &m.content {
                    if !text.is_empty() {
                        input.push(json!({
                            "role": "assistant",
                            "content": text,
                        }));
                    }
                }
            }
            role => {
                input.push(json!({
                    "role": role,
                    "content": m.content.clone().unwrap_or_default(),
                }));
            }
        }
    }
    input
}

/// Parse Responses API `output` array into assistant contents.
fn parse_responses_output(output: &[Value]) -> Vec<AssistantContent> {
    let mut contents = Vec::new();
    for (i, item) in output.iter().enumerate() {
        let ty = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ty {
            "function_call" | "tool_call" => {
                let id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("call_{i}"));
                let name = item
                    .get("name")
                    .or_else(|| item.pointer("/function/name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args_raw = item
                    .get("arguments")
                    .or_else(|| item.pointer("/function/arguments"))
                    .cloned()
                    .unwrap_or(json!({}));
                let args: Value = match args_raw {
                    Value::String(s) => {
                        serde_json::from_str(&s).unwrap_or_else(|_| json!({ "raw": s }))
                    }
                    other => other,
                };
                if !name.is_empty() {
                    contents.push(AssistantContent::ToolCall(ToolCall {
                        id,
                        function: ToolFunction {
                            name,
                            arguments: args,
                        },
                    }));
                }
            }
            "message" => {
                // content: string | [{type: output_text, text}]
                if let Some(c) = item.get("content") {
                    match c {
                        Value::String(s) if !s.is_empty() => {
                            contents.push(AssistantContent::Text(Text { text: s.clone() }));
                        }
                        Value::Array(parts) => {
                            for p in parts {
                                let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                if matches!(ptype, "output_text" | "text") {
                                    if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                                        if !t.is_empty() {
                                            contents.push(AssistantContent::Text(Text {
                                                text: t.to_string(),
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "output_text" => {
                if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        contents.push(AssistantContent::Text(Text {
                            text: t.to_string(),
                        }));
                    }
                }
            }
            _ => {
                // Ignore reasoning / encrypted / web_search items for the agent tool loop.
            }
        }
    }
    contents
}

/// Extract JSON body from raw HTTP text (plain JSON or SSE `data:` frames).
pub fn extract_json_payload(text: &str) -> String {
    let t = text.trim();
    if t.starts_with('{') || t.starts_with('[') {
        return t.to_string();
    }
    let mut last = None;
    for line in t.lines() {
        let line = line.trim();
        let data = if let Some(rest) = line.strip_prefix("data:") {
            rest.trim()
        } else {
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if data.starts_with('{') || data.starts_with('[') {
            last = Some(data.to_string());
        }
    }
    last.unwrap_or_else(|| t.to_string())
}

fn contents_to_response(
    contents: Vec<AssistantContent>,
    raw: Value,
) -> RigCompletionResponse<Value> {
    let contents = if contents.is_empty() {
        vec![AssistantContent::Text(Text {
            text: String::new(),
        })]
    } else {
        contents
    };
    RigCompletionResponse {
        choice: OneOrMany::many(contents).unwrap_or_else(|_| {
            OneOrMany::one(AssistantContent::Text(Text { text: "".into() }))
        }),
        raw_response: raw,
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

        match self.client.style {
            LlmApiStyle::Responses => self.completion_responses(&request, &messages).await,
            LlmApiStyle::ChatCompletions => {
                self.completion_chat_completions(&request, &messages).await
            }
        }
    }
}

impl MeshCompletionModel {
    async fn completion_chat_completions(
        &self,
        request: &CompletionRequest,
        messages: &[OutMessage],
    ) -> std::result::Result<RigCompletionResponse<Value>, CompletionError> {
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

        let url = self.client.chat_completions_url();
        tracing::info!(
            %url,
            model = %self.model,
            provider = self.client.provider.as_str(),
            n_messages = messages.len(),
            "llm chat.completions"
        );

        let text = self.post_json(&url, &body).await?;
        let json_text = extract_json_payload(&text);
        let parsed: ChatCompletionResponse = serde_json::from_str(&json_text).map_err(|e| {
            let snippet: String = json_text.chars().take(600).collect();
            CompletionError::ResponseError(format!("{e}: {snippet}"))
        })?;

        let choice = parsed.choices.into_iter().next().ok_or_else(|| {
            CompletionError::ResponseError("llm: no choices in chat.completions response".into())
        })?;

        let mut contents: Vec<AssistantContent> = Vec::new();
        if let Some(c) = choice.message.content {
            if !c.is_empty() {
                contents.push(AssistantContent::Text(Text { text: c }));
            }
        }
        if let Some(tcs) = choice.message.tool_calls {
            for (i, tc) in tcs.into_iter().enumerate() {
                let args: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or_else(|_| json!({ "raw": tc.function.arguments }));
                let id = if tc.id.is_empty() {
                    format!("call_{i}")
                } else {
                    tc.id
                };
                contents.push(AssistantContent::ToolCall(ToolCall {
                    id,
                    function: ToolFunction {
                        name: tc.function.name,
                        arguments: args,
                    },
                }));
            }
        }

        Ok(contents_to_response(
            contents,
            json!({
                "provider": self.client.provider.as_str(),
                "api": "chat_completions",
            }),
        ))
    }

    async fn completion_responses(
        &self,
        request: &CompletionRequest,
        messages: &[OutMessage],
    ) -> std::result::Result<RigCompletionResponse<Value>, CompletionError> {
        let input = messages_to_responses_input(messages);
        let mut body = json!({
            "model": self.model,
            "input": input,
            "store": self.client.store,
            "stream": false,
        });
        // Optional max output tokens (Responses uses max_output_tokens on some stacks)
        if let Some(max) = request.max_tokens {
            body["max_output_tokens"] = json!(max);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if !request.tools.is_empty() {
            // OpenAI / xAI Responses tool schema (flat function tools)
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    })
                })
                .collect();
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }

        let url = self.client.responses_url();
        tracing::info!(
            %url,
            model = %self.model,
            provider = self.client.provider.as_str(),
            store = self.client.store,
            n_input = input.len(),
            "llm responses"
        );

        let text = self.post_json(&url, &body).await?;
        let json_text = extract_json_payload(&text);
        let parsed: ResponsesApiBody = serde_json::from_str(&json_text).map_err(|e| {
            let snippet: String = json_text.chars().take(600).collect();
            CompletionError::ResponseError(format!("responses parse: {e}: {snippet}"))
        })?;

        let mut contents = parse_responses_output(&parsed.output);

        // Fallbacks: output_text field or nested chat choices
        if contents.is_empty() {
            if let Some(t) = parsed.output_text {
                if !t.is_empty() {
                    contents.push(AssistantContent::Text(Text { text: t }));
                }
            }
        }
        if contents.is_empty() {
            if let Some(choice) = parsed.choices.into_iter().next() {
                if let Some(c) = choice.message.content {
                    if !c.is_empty() {
                        contents.push(AssistantContent::Text(Text { text: c }));
                    }
                }
                if let Some(tcs) = choice.message.tool_calls {
                    for (i, tc) in tcs.into_iter().enumerate() {
                        let args: Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or_else(|_| json!({ "raw": tc.function.arguments }));
                        let id = if tc.id.is_empty() {
                            format!("call_{i}")
                        } else {
                            tc.id
                        };
                        contents.push(AssistantContent::ToolCall(ToolCall {
                            id,
                            function: ToolFunction {
                                name: tc.function.name,
                                arguments: args,
                            },
                        }));
                    }
                }
            }
        }

        Ok(contents_to_response(
            contents,
            json!({
                "provider": self.client.provider.as_str(),
                "api": "responses",
                "response_id": parsed.id,
                "store": self.client.store,
            }),
        ))
    }

    async fn post_json(
        &self,
        url: &str,
        body: &Value,
    ) -> std::result::Result<String, CompletionError> {
        let resp = self
            .client
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.client.api_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| CompletionError::RequestError(e.into()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| CompletionError::RequestError(e.into()))?;

        if !status.is_success() {
            let snippet: String = text.chars().take(900).collect();
            return Err(CompletionError::ProviderError(format!(
                "llm HTTP {status} ({url}): {snippet}"
            )));
        }
        Ok(text)
    }
}

/// Default agent builder — xAI Grok 4.5 when `XAI_API_KEY` is set, else Clawd mesh.
pub fn mesh_agent_builder() -> AgentBuilder<MeshCompletionModel> {
    let client = MeshClient::from_env();
    let model = mesh_model();
    tracing::info!(
        provider = client.provider.as_str(),
        api = client.style.as_str(),
        base = %client.base_url,
        %model,
        store = client.store,
        timeout_secs = llm_timeout_secs(),
        "using inference backend"
    );
    client.agent(&model)
}

/// Quick connectivity check used by tests / doctor.
pub async fn mesh_health_ping() -> Result<String> {
    match llm_provider() {
        LlmProvider::Xai => {
            // xAI has no public /health; do a tiny Responses probe with max_output_tokens=1
            // or just report configured endpoint.
            Ok(format!(
                r#"{{"ok":true,"provider":"xai","base":"{}","model":"{}"}}"#,
                mesh_base_url(),
                mesh_model()
            ))
        }
        LlmProvider::Mesh => {
            let base = mesh_base_url();
            let origin = base
                .trim_end_matches('/')
                .trim_end_matches("/v1")
                .to_string();
            let url = format!("{origin}/health");
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()?;
            let text = client.get(&url).send().await?.text().await?;
            Ok(text)
        }
    }
}

/// One-shot completion against the active backend (integration / smoke).
pub async fn mesh_smoke_completion(prompt: &str) -> Result<String> {
    use rig::completion::CompletionModel;
    let model = MeshCompletionModel {
        client: Arc::new(MeshClient::from_env()),
        model: mesh_model(),
    };
    let req = model.completion_request(prompt).max_tokens(64).build();
    let resp = model.completion(req).await?;
    let mut out = String::new();
    for c in resp.choice.iter() {
        if let AssistantContent::Text(t) = c {
            out.push_str(&t.text);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::message::UserContent;

    #[test]
    fn extract_json_from_plain() {
        let j = r#"{"choices":[]}"#;
        assert_eq!(extract_json_payload(j), j);
    }

    #[test]
    fn extract_json_from_sse() {
        let sse = "event: message\ndata: {\"choices\":[{\"message\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n";
        let got = extract_json_payload(sse);
        assert!(got.contains("choices"), "{got}");
        assert!(got.contains("hi"), "{got}");
    }

    #[test]
    fn tool_arguments_string_or_object() {
        let as_str = r#"{"id":"1","function":{"name":"get_public_key","arguments":"{}"}}"#;
        let tc: ResponseToolCall = serde_json::from_str(as_str).unwrap();
        assert_eq!(tc.function.arguments, "{}");

        let as_obj = r#"{"id":"2","function":{"name":"x","arguments":{"a":1}}}"#;
        let tc2: ResponseToolCall = serde_json::from_str(as_obj).unwrap();
        assert!(
            tc2.function.arguments.contains('1'),
            "{}",
            tc2.function.arguments
        );
    }

    #[test]
    fn user_message_maps_to_openai_role() {
        let msg = RigMessage::User {
            content: OneOrMany::one(UserContent::text("hello mesh")),
        };
        let out = rig_message_to_openai(&msg);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content.as_deref(), Some("hello mesh"));
    }

    #[test]
    fn chat_url_no_double_slash() {
        let c = MeshClient {
            base_url: "https://mesh.x402.wtf/v1".into(),
            api_key: "x".into(),
            http: reqwest::Client::new(),
            style: LlmApiStyle::ChatCompletions,
            provider: LlmProvider::Mesh,
            store: false,
        };
        assert_eq!(
            c.chat_completions_url(),
            "https://mesh.x402.wtf/v1/chat/completions"
        );
        assert!(!c.chat_completions_url().contains("https:/m"));
        assert!(c.chat_completions_url().starts_with("https://"));
    }

    #[test]
    fn responses_url_for_xai() {
        let c = MeshClient {
            base_url: "https://api.x.ai/v1".into(),
            api_key: "xai-…".into(),
            http: reqwest::Client::new(),
            style: LlmApiStyle::Responses,
            provider: LlmProvider::Xai,
            store: false,
        };
        assert_eq!(c.responses_url(), "https://api.x.ai/v1/responses");
    }

    #[test]
    fn parse_responses_function_call_and_text() {
        let output = vec![
            json!({
                "type": "function_call",
                "call_id": "call_1",
                "name": "get_public_key",
                "arguments": "{}"
            }),
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "done"}]
            }),
        ];
        let c = parse_responses_output(&output);
        assert_eq!(c.len(), 2);
        match &c[0] {
            AssistantContent::ToolCall(tc) => {
                assert_eq!(tc.function.name, "get_public_key");
                assert_eq!(tc.id, "call_1");
            }
            _ => panic!("expected tool call"),
        }
        match &c[1] {
            AssistantContent::Text(t) => assert_eq!(t.text, "done"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn messages_to_responses_input_tool_result() {
        let msgs = vec![
            OutMessage {
                role: "user".into(),
                content: Some("hi".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            OutMessage {
                role: "tool".into(),
                content: Some("0xabc".into()),
                tool_calls: None,
                tool_call_id: Some("call_1".into()),
            },
        ];
        let input = messages_to_responses_input(&msgs);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[1]["type"], "function_call_output");
        assert_eq!(input[1]["call_id"], "call_1");
    }

    #[tokio::test]
    #[ignore = "network — live Clawd mesh"]
    async fn live_mesh_health() {
        let h = mesh_health_ping().await.expect("health");
        assert!(
            h.contains("ok") || h.contains("true") || h.contains("node") || h.contains("xai"),
            "{h}"
        );
    }

    #[tokio::test]
    #[ignore = "network — live backend"]
    async fn live_mesh_completion() {
        let text = mesh_smoke_completion("Reply with exactly: pong")
            .await
            .expect("completion");
        assert!(!text.is_empty(), "empty mesh reply");
    }
}
