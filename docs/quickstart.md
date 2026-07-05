# Quick Start

Set the minimum environment:

```bash
export ANTHROPIC_API_KEY=...
export SOLANA_PRIVATE_KEY=...
export SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
```

Create an SVM-aware Solana agent inside a signer scope:

```rust
use std::sync::Arc;

use openclawd_solana_kit::signer::solana::LocalSolanaSigner;
use openclawd_solana_kit::signer::SignerContext;
use openclawd_solana_kit::solana::agent::create_solana_agent;
use rig::completion::Prompt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = LocalSolanaSigner::new(std::env::var("SOLANA_PRIVATE_KEY")?);

    SignerContext::with_signer(Arc::new(signer), async {
        let agent = create_solana_agent(None).await?;
        let response = agent.prompt("what is my public key?").await?;
        println!("{response}");
        Ok(())
    })
    .await
}
```

Run the included examples:

```bash
cargo run --example simple
cargo run --example solana_agent
```

Run the HTTP service:

```bash
cargo run --features full --bin kit
```
