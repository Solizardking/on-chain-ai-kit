# SignerContext

`SignerContext` binds one `TransactionSigner` to the current async scope. Solana
tools use that signer for transaction execution instead of reading a global key.

```rust
SignerContext::with_signer(Arc::new(signer), async {
    // Tool calls inside this block use only this signer.
    Ok(())
})
.await?;
```

## Why It Exists

Agent systems are concurrent. A web service can process many users at once, and
each user needs a separate signing identity. `SignerContext` keeps those
identities scoped to the request that created them.

## Trait Shape

```rust
#[async_trait]
pub trait TransactionSigner: Send + Sync {
    fn pubkey(&self) -> String;

    async fn sign_and_send_solana_transaction(
        &self,
        tx: &mut solana_sdk::transaction::Transaction,
    ) -> anyhow::Result<String>;

    async fn sign_and_send_encoded_solana_transaction(
        &self,
        tx: String,
    ) -> anyhow::Result<String>;
}
```

Built-in signers:

- `LocalSolanaSigner` for local development
- `PrivySigner` for HTTP service deployments with delegated signing

Custom signers can wrap KMS, MPC, wallet infrastructure, or sponsored
transaction providers as long as they implement `TransactionSigner`.
