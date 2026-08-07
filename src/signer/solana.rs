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

    /// Parse a base58 Solana secret key (64-byte keypair encoding, or 32-byte seed).
    pub fn try_new(private_key: impl AsRef<str>) -> Result<Self> {
        let raw = private_key.as_ref().trim();
        if raw.is_empty() {
            anyhow::bail!("SOLANA_PRIVATE_KEY is empty");
        }
        let lower = raw.to_ascii_lowercase();
        if raw == "your_key_here"
            || lower == "changeme"
            || lower == "todo"
            || raw.starts_with("PLACEHOLDER")
            || raw.starts_with("xxx")
        {
            anyhow::bail!(
                "SOLANA_PRIVATE_KEY looks like a placeholder — set a real base58 secret keypair"
            );
        }
        // Validate base58 charset (no `.` in alphabet — scanners' "..." never match real keys).
        if !raw
            .chars()
            .all(|c| matches!(c, '1'..='9' | 'A'..='H' | 'J'..='N' | 'P'..='Z' | 'a'..='k' | 'm'..='z'))
        {
            anyhow::bail!(
                "SOLANA_PRIVATE_KEY is not valid base58 (got invalid characters or placeholder)"
            );
        }
        // Keypair::from_base58_string panics on bad length/decode in some solana versions.
        let keypair = std::panic::catch_unwind(|| Keypair::from_base58_string(raw))
            .map_err(|_| {
                anyhow::anyhow!(
                    "SOLANA_PRIVATE_KEY failed to decode as a Solana keypair \
                     (need base58 of 64-byte secret key from `solana-keygen new --no-outfile`)"
                )
            })?;
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
