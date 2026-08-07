use super::middleware::verify_auth;
use super::state::{AppState, AuthMode};
use crate::common::spawn_with_signer;
#[cfg(feature = "cross-chain")]
use crate::cross_chain::agent::create_cross_chain_agent;
#[cfg(feature = "evm")]
use crate::evm::agent::create_evm_agent;
use crate::reasoning_loop::LoopResponse;
use crate::reasoning_loop::ReasoningLoop;
use crate::signer::privy::PrivySigner;
use crate::signer::TransactionSigner;
#[cfg(feature = "solana")]
use crate::solana::agent::create_solana_agent;
use actix_web::{get, post, web, Error, HttpRequest, HttpResponse, Responder};
use actix_web_lab::sse;
use rig::completion::Message;
use rig::message::UserContent;
use rig::OneOrMany;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[derive(Deserialize)]
pub struct ChatRequest {
    prompt: String,
    #[serde(deserialize_with = "deserialize_messages")]
    chat_history: Vec<Message>,
    #[serde(default)]
    chain: Option<String>,
    #[serde(default)]
    preamble: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "content")]
pub enum StreamResponse {
    Message(String),
    ToolCall { name: String, result: String },
    Error(String),
}

async fn build_signer(
    state: &AppState,
    req: &HttpRequest,
) -> Result<Arc<dyn TransactionSigner>, String> {
    match state.auth_mode {
        AuthMode::Local => {
            #[cfg(feature = "solana")]
            {
                state
                    .local_signer
                    .clone()
                    .map(|s| s as Arc<dyn TransactionSigner>)
                    .ok_or_else(|| "Local signer not configured".into())
            }
            #[cfg(not(feature = "solana"))]
            {
                Err("solana feature disabled".into())
            }
        }
        AuthMode::Privy => {
            let session = verify_auth(req)
                .await
                .map_err(|e| format!("unauthorized: {e}"))?;
            let privy = state
                .privy
                .clone()
                .ok_or_else(|| "Privy client not configured".to_string())?;
            Ok(Arc::new(PrivySigner::new(privy, session)) as Arc<dyn TransactionSigner>)
        }
    }
}

#[post("/stream")]
async fn stream(
    req: HttpRequest,
    state: web::Data<AppState>,
    request: web::Json<ChatRequest>,
) -> impl Responder {
    let (tx, rx) = tokio::sync::mpsc::channel::<sse::Event>(32);

    let send_err = |tx: &tokio::sync::mpsc::Sender<sse::Event>, msg: String| {
        let error_event = sse::Event::Data(sse::Data::new(
            serde_json::to_string(&StreamResponse::Error(msg)).unwrap(),
        ));
        let _ = tx.try_send(error_event);
    };

    let signer = match build_signer(state.get_ref(), &req).await {
        Ok(s) => s,
        Err(e) => {
            send_err(&tx, format!("Error: {e}"));
            return sse::Sse::from_infallible_receiver(rx)
                .with_keep_alive(Duration::from_secs(15))
                .with_retry_duration(Duration::from_secs(10));
        }
    };

    let preamble = request.preamble.clone();

    let agent = match request.chain.as_deref() {
        #[cfg(feature = "solana")]
        Some("solana") | None => match create_solana_agent(preamble).await {
            Ok(agent) => Arc::new(agent),
            Err(e) => {
                send_err(&tx, format!("Failed to create Solana agent: {e}"));
                return sse::Sse::from_infallible_receiver(rx)
                    .with_keep_alive(Duration::from_secs(15))
                    .with_retry_duration(Duration::from_secs(10));
            }
        },
        #[cfg(feature = "evm")]
        Some("evm") => match create_evm_agent(preamble).await {
            Ok(agent) => Arc::new(agent),
            Err(e) => {
                send_err(&tx, format!("Failed to create EVM agent: {e}"));
                return sse::Sse::from_infallible_receiver(rx)
                    .with_keep_alive(Duration::from_secs(15))
                    .with_retry_duration(Duration::from_secs(10));
            }
        },
        #[cfg(feature = "cross-chain")]
        Some("omni") => match create_cross_chain_agent(preamble).await {
            Ok(agent) => Arc::new(agent),
            Err(e) => {
                send_err(&tx, format!("Failed to create cross-chain agent: {e}"));
                return sse::Sse::from_infallible_receiver(rx)
                    .with_keep_alive(Duration::from_secs(15))
                    .with_retry_duration(Duration::from_secs(10));
            }
        },
        Some(chain) => {
            send_err(&tx, format!("Unsupported chain: {chain}"));
            return sse::Sse::from_infallible_receiver(rx)
                .with_keep_alive(Duration::from_secs(15))
                .with_retry_duration(Duration::from_secs(10));
        }
        #[cfg(not(feature = "solana"))]
        None => {
            send_err(
                &tx,
                "No default chain is enabled. Rebuild with the solana feature.".into(),
            );
            return sse::Sse::from_infallible_receiver(rx)
                .with_keep_alive(Duration::from_secs(15))
                .with_retry_duration(Duration::from_secs(10));
        }
    };

    let prompt = request.prompt.clone();
    let messages = request.chat_history.clone();
    tracing::info!(
        auth_mode = state.auth_mode.as_str(),
        prompt = %prompt,
        "stream request"
    );

    spawn_with_signer(signer, || async move {
        let reasoning_loop = ReasoningLoop::new(agent).with_stdout(false);

        let mut initial_messages = messages;
        initial_messages.push(Message::User {
            content: OneOrMany::one(UserContent::text(prompt)),
        });

        let (internal_tx, mut internal_rx) = tokio::sync::mpsc::channel(32);

        let tx_clone = tx.clone();
        let send_task = tokio::spawn(async move {
            while let Some(response) = internal_rx.recv().await {
                let stream_response = match response {
                    LoopResponse::Message(text) => StreamResponse::Message(text),
                    LoopResponse::ToolCall { name, result } => {
                        StreamResponse::ToolCall { name, result }
                    }
                };

                if tx_clone
                    .send(sse::Event::Data(sse::Data::new(
                        serde_json::to_string(&stream_response).unwrap(),
                    )))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let loop_result = reasoning_loop
            .stream(initial_messages, Some(internal_tx))
            .await;

        let _ = send_task.await;

        if let Err(e) = loop_result {
            let _ = tx
                .send(sse::Event::Data(sse::Data::new(
                    serde_json::to_string(&StreamResponse::Error(e.to_string())).unwrap(),
                )))
                .await;
        }

        Ok(())
    })
    .await;

    sse::Sse::from_infallible_receiver(rx)
        .with_keep_alive(Duration::from_secs(15))
        .with_retry_duration(Duration::from_secs(10))
}

#[get("/healthz")]
async fn healthz(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    #[cfg(feature = "solana")]
    let pubkey = {
        use crate::signer::TransactionSigner;
        state
            .local_signer
            .as_ref()
            .map(|s| s.pubkey())
            .unwrap_or_default()
    };
    #[cfg(not(feature = "solana"))]
    let pubkey = String::new();

    Ok(HttpResponse::Ok().json(json!({
        "status": "ok",
        "auth_mode": state.auth_mode.as_str(),
        "local_pubkey": if pubkey.is_empty() { serde_json::Value::Null } else { json!(pubkey) },
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

#[get("/auth")]
async fn auth(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    match state.auth_mode {
        AuthMode::Local => {
            #[cfg(feature = "solana")]
            {
                use crate::signer::TransactionSigner;
                let pk = state
                    .local_signer
                    .as_ref()
                    .map(|s| s.pubkey())
                    .unwrap_or_default();
                Ok(HttpResponse::Ok().json(json!({
                    "status": "ok",
                    "auth_mode": "local",
                    "wallet_address": pk,
                    "note": "No Privy — kit signs with SOLANA_PRIVATE_KEY. Dev only; do not expose publicly."
                })))
            }
            #[cfg(not(feature = "solana"))]
            {
                let _ = req;
                Ok(HttpResponse::ServiceUnavailable().json(json!({
                    "error": "solana feature disabled"
                })))
            }
        }
        AuthMode::Privy => {
            let user_session = match verify_auth(&req).await {
                Ok(session) => session,
                Err(e) => {
                    return Ok(HttpResponse::Unauthorized().json(json!({ "error": e.to_string() })))
                }
            };
            Ok(HttpResponse::Ok().json(json!({
                "status": "ok",
                "auth_mode": "privy",
                "wallet_address": user_session.wallet_address,
            })))
        }
    }
}

fn deserialize_messages<'de, D>(deserializer: D) -> Result<Vec<Message>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct RawMessage {
        role: String,
        content: serde_json::Value,
    }

    let raw_messages: Vec<RawMessage> = Vec::deserialize(deserializer)?;

    raw_messages
        .into_iter()
        .map(|raw| {
            let content = match raw.role.as_str() {
                "user" => {
                    let content = match raw.content {
                        serde_json::Value::String(s) => OneOrMany::one(UserContent::Text(s.into())),
                        _ => return Err(serde::de::Error::custom("Invalid user content format")),
                    };
                    Message::User { content }
                }
                "assistant" => {
                    let content = match raw.content {
                        serde_json::Value::String(s) => OneOrMany::one(s.into()),
                        _ => {
                            return Err(serde::de::Error::custom(
                                "Invalid assistant content format",
                            ))
                        }
                    };
                    Message::Assistant { content }
                }
                _ => return Err(serde::de::Error::custom("Invalid role")),
            };
            Ok(content)
        })
        .collect()
}
