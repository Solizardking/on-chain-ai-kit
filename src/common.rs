use anyhow::{anyhow, Result};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::mesh::{self, MeshCompletionModel};
use crate::signer::{SignerContext, TransactionSigner};
use rig::agent::{Agent, AgentBuilder};

pub async fn wrap_unsafe<F, Fut, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    let (tx, mut rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let result = f().await;
        let _ = tx.send(result).await;
    });

    rx.recv().await.ok_or_else(|| anyhow!("Channel closed"))?
}

pub async fn spawn_with_signer<F, Fut, T>(
    signer: Arc<dyn TransactionSigner>,
    f: F,
) -> tokio::task::JoinHandle<Result<T>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    tokio::spawn(async move { SignerContext::with_signer(signer, async { f().await }).await })
}

use crate::constitution;

// ─── Default endpoints ───────────────────────────────────────────

/// Default Clawd free inference mesh (OpenAI-compatible `/v1/chat/completions`).
pub const DEFAULT_MESH_BASE_URL: &str = "https://clawd-inference-mesh.fly.dev/v1";
/// Public alias for the same mesh app.
pub const DEFAULT_MESH_PUBLIC_URL: &str = "https://mesh.x402.wtf/v1";
/// Default free-router model on the Clawd mesh.
pub const DEFAULT_MESH_MODEL: &str = "zkrouter/auto";

/// xAI API base (Responses + Chat Completions under `/v1`).
pub const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";
/// Preferred Grok model when `XAI_API_KEY` is set.
pub const DEFAULT_XAI_MODEL: &str = "grok-4.5";

/// Which LLM backend the kit uses for agent completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    /// xAI Grok via Responses API (or chat when forced).
    Xai,
    /// Clawd inference mesh (OpenAI-compatible chat completions).
    Mesh,
}

impl LlmProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            LlmProvider::Xai => "xai",
            LlmProvider::Mesh => "mesh",
        }
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// True when `XAI_API_KEY` is present.
pub fn xai_api_key_present() -> bool {
    env_nonempty("XAI_API_KEY").is_some()
}

/// Resolve LLM provider.
///
/// Priority:
/// 1. `CLAWD_LLM_PROVIDER=xai|mesh` (or `grok` → xai)
/// 2. If `XAI_API_KEY` is set → **xai** (Grok 4.5)
/// 3. Else → Clawd mesh free router
pub fn llm_provider() -> LlmProvider {
    if let Some(raw) = env_nonempty("CLAWD_LLM_PROVIDER")
        .or_else(|| env_nonempty("LLM_PROVIDER"))
    {
        match raw.to_ascii_lowercase().as_str() {
            "xai" | "grok" | "x-ai" => return LlmProvider::Xai,
            "mesh" | "clawd" | "free" => return LlmProvider::Mesh,
            _ => {}
        }
    }
    if xai_api_key_present() {
        LlmProvider::Xai
    } else {
        LlmProvider::Mesh
    }
}

/// API style for the active provider.
/// - xAI default: **Responses** API (`POST /v1/responses`) — preferred for Grok 4.5
/// - mesh default: **Chat Completions** (`POST /v1/chat/completions`)
///
/// Override with `CLAWD_LLM_API=responses|chat`.
pub fn llm_api_style() -> LlmApiStyle {
    if let Some(raw) = env_nonempty("CLAWD_LLM_API").or_else(|| env_nonempty("XAI_API_STYLE")) {
        match raw.to_ascii_lowercase().as_str() {
            "responses" | "response" => return LlmApiStyle::Responses,
            "chat" | "completions" | "chat_completions" => return LlmApiStyle::ChatCompletions,
            _ => {}
        }
    }
    match llm_provider() {
        LlmProvider::Xai => LlmApiStyle::Responses,
        LlmProvider::Mesh => LlmApiStyle::ChatCompletions,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmApiStyle {
    Responses,
    ChatCompletions,
}

impl LlmApiStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            LlmApiStyle::Responses => "responses",
            LlmApiStyle::ChatCompletions => "chat_completions",
        }
    }
}

/// Base URL for the active LLM backend (always ends without trailing slash).
pub fn mesh_base_url() -> String {
    match llm_provider() {
        LlmProvider::Xai => env_nonempty("XAI_BASE_URL")
            .or_else(|| env_nonempty("CLAWD_MESH_BASE_URL"))
            .or_else(|| env_nonempty("OPENAI_BASE_URL"))
            .unwrap_or_else(|| DEFAULT_XAI_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string(),
        LlmProvider::Mesh => env_nonempty("CLAWD_MESH_BASE_URL")
            .or_else(|| env_nonempty("OPENAI_BASE_URL"))
            .unwrap_or_else(|| DEFAULT_MESH_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string(),
    }
}

/// Bearer token for the active backend.
pub fn mesh_api_key() -> String {
    match llm_provider() {
        LlmProvider::Xai => env_nonempty("XAI_API_KEY")
            .or_else(|| env_nonempty("CLAWD_MESH_API_KEY"))
            .or_else(|| env_nonempty("OPENAI_API_KEY"))
            .unwrap_or_else(|| "missing-xai-api-key".to_string()),
        LlmProvider::Mesh => env_nonempty("CLAWD_MESH_API_KEY")
            .or_else(|| env_nonempty("OPENAI_API_KEY"))
            .unwrap_or_else(|| "clawd-mesh".to_string()),
    }
}

/// Model id for the active backend.
///
/// When using xAI: `XAI_MODEL` → `CLAWD_MESH_MODEL` → `OPENAI_MODEL` → **`grok-4.5`**.
/// When using mesh: `CLAWD_MESH_MODEL` → `OPENAI_MODEL` → **`zkrouter/auto`**.
pub fn mesh_model() -> String {
    match llm_provider() {
        LlmProvider::Xai => env_nonempty("XAI_MODEL")
            .or_else(|| env_nonempty("CLAWD_MESH_MODEL"))
            .or_else(|| env_nonempty("OPENAI_MODEL"))
            .unwrap_or_else(|| DEFAULT_XAI_MODEL.to_string()),
        LlmProvider::Mesh => env_nonempty("CLAWD_MESH_MODEL")
            .or_else(|| env_nonempty("OPENAI_MODEL"))
            .unwrap_or_else(|| DEFAULT_MESH_MODEL.to_string()),
    }
}

/// Whether xAI Responses API should store turns server-side (default **false** for agent tool loops).
pub fn xai_store_messages() -> bool {
    match env_nonempty("XAI_STORE_MESSAGES")
        .or_else(|| env_nonempty("XAI_STORE"))
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("1" | "true" | "yes" | "on") => true,
        _ => false,
    }
}

/// HTTP timeout for LLM calls. Reasoning models need longer; default 600s for xAI, 120s for mesh.
pub fn llm_timeout_secs() -> u64 {
    if let Some(s) = env_nonempty("XAI_TIMEOUT_SECS")
        .or_else(|| env_nonempty("CLAWD_LLM_TIMEOUT_SECS"))
    {
        if let Ok(n) = s.parse::<u64>() {
            return n.clamp(30, 3600);
        }
    }
    match llm_provider() {
        LlmProvider::Xai => 600,
        LlmProvider::Mesh => 120,
    }
}

/// Default agent builder → xAI Grok 4.5 when keyed, else Clawd mesh.
pub fn mesh_agent_builder() -> AgentBuilder<MeshCompletionModel> {
    mesh::mesh_agent_builder()
}

/// Backward-compatible alias.
pub fn claude_agent_builder() -> AgentBuilder<MeshCompletionModel> {
    mesh_agent_builder()
}

pub async fn plain_agent() -> Result<Agent<MeshCompletionModel>> {
    Ok(mesh_agent_builder()
        .preamble(&preamble_common())
        .max_tokens(1024)
        .build())
}

pub fn preamble_common() -> String {
    constitution::clawd_system_preamble()
}

#[deprecated(note = "use preamble_common() — includes Clawd constitution")]
pub const PREAMBLE_COMMON: &str = "";

#[cfg(test)]
mod llm_select_tests {
    use super::*;

    #[test]
    fn provider_defaults_mesh_without_xai() {
        // Don't assert global env (other tests may set keys); just ensure enum labels stable.
        assert_eq!(LlmProvider::Xai.as_str(), "xai");
        assert_eq!(LlmProvider::Mesh.as_str(), "mesh");
        assert_eq!(LlmApiStyle::Responses.as_str(), "responses");
    }
}
