use actix_web::{web, HttpRequest};
use anyhow::Result;
use privy::auth::UserSession;

use super::state::{AppState, AuthMode};

/// Authenticate a Privy Bearer token. Only used when `AuthMode::Privy`.
pub async fn verify_auth(req: &HttpRequest) -> Result<UserSession> {
    let state = req
        .app_data::<web::Data<AppState>>()
        .ok_or_else(|| anyhow::anyhow!("App state not found"))?;

    if state.auth_mode != AuthMode::Privy {
        return Err(anyhow::anyhow!(
            "Privy auth not active (KIT_AUTH_MODE={})",
            state.auth_mode.as_str()
        ));
    }

    let privy = state
        .privy
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Privy client not configured"))?;

    let token = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("Missing authorization header"))?;

    let token = token
        .strip_prefix("Bearer ")
        .ok_or_else(|| anyhow::anyhow!("Invalid authorization format"))?;

    match privy.authenticate_user(token).await {
        Ok(session) => Ok(session),
        Err(e) => Err(anyhow::anyhow!("Authentication failed: {}", e)),
    }
}
