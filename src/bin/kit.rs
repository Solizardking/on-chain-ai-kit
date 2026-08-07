#[cfg(feature = "http")]
use {
    openclawd_solana_kit::env_load::{env_load_hint, load_dotenv_files},
    openclawd_solana_kit::http::server::run_server,
    openclawd_solana_kit::http::state::AppState,
};

#[cfg(feature = "http")]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let loaded = load_dotenv_files();
    if !loaded.is_empty() {
        eprintln!(
            "openclawd-kit: loaded env from {}",
            loaded
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Default: local mode (SOLANA_PRIVATE_KEY) — no Privy required.
    // Optional: KIT_AUTH_MODE=privy + PRIVY_* for multi-user delegated signing.
    let state = AppState::from_env().map_err(|e| {
        eprintln!("openclawd-kit: failed to start: {e}\n{}", env_load_hint());
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    eprintln!(
        "openclawd-kit: starting SSE service on 0.0.0.0:6969 (auth_mode={})",
        state.auth_mode.as_str()
    );
    run_server(state).await
}

#[cfg(not(feature = "http"))]
fn main() {
    eprintln!(
        "This binary requires the 'http' feature.\n\
         Run: cargo run --features full --bin kit\n\
         Or:  npm run kit"
    );
    std::process::exit(1);
}
