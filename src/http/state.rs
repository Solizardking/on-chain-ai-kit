use std::sync::Arc;

#[cfg(feature = "solana")]
use crate::signer::solana::LocalSolanaSigner;
#[cfg(feature = "solana")]
use crate::signer::TransactionSigner;

/// How the HTTP kit authenticates and signs.
///
/// Default is **local**: no Privy — `SOLANA_PRIVATE_KEY`, open `/stream` (dev only).
/// Set `KIT_AUTH_MODE=privy` (+ Privy env) for multi-user delegated signing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Local,
    Privy,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::Local => "local",
            AuthMode::Privy => "privy",
        }
    }
}

pub struct AppState {
    pub auth_mode: AuthMode,
    #[cfg(feature = "solana")]
    pub local_signer: Option<Arc<LocalSolanaSigner>>,
    pub privy: Option<Arc<privy::Privy>>,
}

impl AppState {
    /// Build state from environment after dotenv load.
    pub fn from_env() -> anyhow::Result<Self> {
        let mode_raw = std::env::var("KIT_AUTH_MODE")
            .unwrap_or_else(|_| "local".into())
            .to_lowercase();

        match mode_raw.as_str() {
            "privy" => Self::privy_from_env(),
            "local" | "" => Self::local_from_env(),
            other => anyhow::bail!(
                "Unknown KIT_AUTH_MODE={other:?}. Use `local` (default, no Privy) or `privy`."
            ),
        }
    }

    fn local_from_env() -> anyhow::Result<Self> {
        #[cfg(feature = "solana")]
        {
            let key = std::env::var("SOLANA_PRIVATE_KEY").map_err(|_| {
                anyhow::anyhow!(
                    "Local kit mode requires SOLANA_PRIVATE_KEY \
                     (or set KIT_AUTH_MODE=privy with PRIVY_* env)"
                )
            })?;
            if key.trim().is_empty() {
                anyhow::bail!("SOLANA_PRIVATE_KEY is empty");
            }
            // Ensure RPC URL for blockhash cache / txs
            if std::env::var("SOLANA_RPC_URL").is_err() {
                std::env::set_var(
                    "SOLANA_RPC_URL",
                    "https://api.mainnet-beta.solana.com",
                );
            }
            let local = LocalSolanaSigner::try_new(key).map_err(|e| {
                anyhow::anyhow!(
                    "{e}. Generate one with: solana-keygen new --no-outfile --no-bip39-passphrase  (or use a funded dev key)"
                )
            })?;
            eprintln!(
                "openclawd-kit: auth_mode=local · pubkey={} · /stream open (no Bearer)",
                local.pubkey()
            );
            Ok(Self {
                auth_mode: AuthMode::Local,
                local_signer: Some(Arc::new(local)),
                privy: None,
            })
        }
        #[cfg(not(feature = "solana"))]
        {
            anyhow::bail!("Local mode requires the solana feature");
        }
    }

    fn privy_from_env() -> anyhow::Result<Self> {
        use privy::{config::PrivyConfig, Privy};

        let cfg = PrivyConfig::from_env().map_err(|e| anyhow::anyhow!("{e}"))?;
        let privy = Privy::new(cfg);
        eprintln!("openclawd-kit: auth_mode=privy · Bearer JWT required on /stream");
        Ok(Self {
            auth_mode: AuthMode::Privy,
            #[cfg(feature = "solana")]
            local_signer: None,
            privy: Some(Arc::new(privy)),
        })
    }
}
