#[cfg(feature = "http")]
use {
    openclawd_solana_kit::http::server::run_server,
    privy::{config::PrivyConfig, Privy},
};

#[cfg(feature = "http")]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let privy_client = Privy::new(
        PrivyConfig::from_env().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
    );

    run_server(privy_client).await
}

#[cfg(not(feature = "http"))]
fn main() {
    println!("This binary requires the 'http' feature");
}
