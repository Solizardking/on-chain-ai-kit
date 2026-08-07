#[cfg(feature = "http")]
use {
    openclawd_solana_kit::env_load::{env_load_hint, load_dotenv_files, privy_env_ready},
    openclawd_solana_kit::http::server::run_server,
    privy::{config::PrivyConfig, Privy},
};

#[cfg(feature = "http")]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Ensure env is loaded even if the binary is invoked before lib ctor paths
    // match the caller's layout (e.g. npm CLI from another cwd).
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

    if !privy_env_ready() {
        eprintln!("openclawd-kit: PRIVY_* environment not ready.\n{}", env_load_hint());
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Missing PRIVY_APP_ID / PRIVY_APP_SECRET / PRIVY_VERIFICATION_KEY — see docs/configuration.md",
        ));
    }

    let privy_client = Privy::new(PrivyConfig::from_env().map_err(|e| {
        eprintln!("openclawd-kit: {}\n{}", e, env_load_hint());
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?);

    eprintln!("openclawd-kit: starting SSE service on 0.0.0.0:6969");
    run_server(privy_client).await
}

#[cfg(not(feature = "http"))]
fn main() {
    eprintln!(
        "This binary requires the 'http' feature.\n\
         Run: cargo run --features full --bin kit\n\
         Or:  npm run kit\n\
         Or:  npx openclawd-solana-kit start"
    );
    std::process::exit(1);
}
