#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "solana")]
pub mod solana;

#[cfg(feature = "evm")]
pub mod evm;

#[cfg(feature = "cross-chain")]
pub mod cross_chain;

pub mod common;
pub mod constitution;
pub mod data;
pub mod dexscreener;
pub mod env_load;
pub mod mesh;
pub mod reasoning_loop;
pub mod signer;

#[ctor::ctor]
fn init() {
    // Prefer multi-path load (.env, .env.local, src/.env.local, CLAWD_ENV_FILE)
    // over bare dotenv::dotenv() which only looks for CWD/.env.
    let _ = env_load::load_dotenv_files();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info")
                    .add_directive("openclawd_solana_kit=info".parse().unwrap())
            }),
        )
        .with_test_writer()
        .try_init()
        .ok();
}
