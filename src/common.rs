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

/// Default Clawd inference mesh (OpenAI-compatible `/v1/chat/completions`).
pub const DEFAULT_MESH_BASE_URL: &str = "https://clawd-inference-mesh.fly.dev/v1";
/// Public alias for the same mesh app.
pub const DEFAULT_MESH_PUBLIC_URL: &str = "https://mesh.x402.wtf/v1";
/// Default free-router model on the mesh.
pub const DEFAULT_MESH_MODEL: &str = "zkrouter/auto";

pub fn mesh_base_url() -> String {
    std::env::var("CLAWD_MESH_BASE_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| DEFAULT_MESH_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn mesh_api_key() -> String {
    std::env::var("CLAWD_MESH_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .unwrap_or_else(|_| "clawd-mesh".to_string())
}

pub fn mesh_model() -> String {
    std::env::var("CLAWD_MESH_MODEL")
        .or_else(|_| std::env::var("OPENAI_MODEL"))
        .unwrap_or_else(|_| DEFAULT_MESH_MODEL.to_string())
}

/// Default agent builder → Clawd inference mesh.
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
