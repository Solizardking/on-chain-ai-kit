use anyhow::Result;
use async_trait::async_trait;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use std::sync::Arc;

use crate::solana::transaction::send_tx;
use blockhash_cache::BLOCKHASH_CACHE;

use super::TransactionSigner;

pub struct LocalSolanaSigner {
    keypair: Arc<Keypair>,
}

impl LocalSolanaSigner {
    pub fn new(private_key: String) -> Self {
        Self::try_new(private_key).unwrap_or_else(|e| panic!("LocalSolanaSigner: {e}"))
    }

    /// Parse a base58 Solana secret key (64-byte keypair encoding).
    pub fn try_new(private_key: impl AsRef<str>) -> Result<Self> {
        let raw = private_key.as_ref().trim();
        if raw.is_empty() {
            anyhow::bail!("SOLANA_PRIVATE_KEY is empty");
        }
        if raw.starts_with('.') || raw.contains("...") || raw == "your_key_here" {
            anyhow::bail!(
                "SOLANA_PRIVATE_KEY looks like a placeholder — set a real base58 secret keypair"
            );
        }
        // Keypair::from_base58_string panics on bad input in some solana versions;
        // decode via bs58-like path: try/catch using std::panic::catch_unwind is heavy —
        // validate characters first.
        if !raw
            .chars()
            .all(|c| matches!(c, '1'..='9' | 'A'..='H' | 'J'..='N' | 'P'..='Z' | 'a'..='k' | 'm'..='z'))
        {
            anyhow::bail!(
                "SOLANA_PRIVATE_KEY is not valid base58 (got invalid characters or placeholder)"
            );
        }
        let keypair = Keypair::from_base58_string(raw);
        Ok(Self {
            keypair: Arc::new(keypair),
        })
    }
}

#[async_trait]
impl TransactionSigner for LocalSolanaSigner {
    fn address(&self) -> String {
        self.keypair.pubkey().to_string()
    }

    fn pubkey(&self) -> String {
        self.keypair.pubkey().to_string()
    }

    async fn sign_and_send_solana_transaction(
        &self,
        tx: &mut solana_sdk::transaction::Transaction,
    ) -> Result<String> {
        let recent_blockhash = BLOCKHASH_CACHE.get_blockhash().await?;
        tx.try_sign(&[&*self.keypair], recent_blockhash)?;
        send_tx(tx).await
    }
}
