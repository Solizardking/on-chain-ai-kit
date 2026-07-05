# OpenClawd SVM Kit

SVM-native Rust tools and signer context for OpenClawd agents.

This crate wraps common Solana actions as `rig-core` tools so an agent can
inspect balances, fetch token context, prepare swaps, transfer assets, and work
with Pump.fun while all signing stays scoped to the active `SignerContext`.

## What It Includes

- SVM agent builder powered by `rig-core`
- Jupiter swap transaction creation
- SOL and SPL token transfers
- SOL and SPL balance checks
- Portfolio lookup and token price helpers
- Pump.fun deploy, buy, and sell helpers
- DexScreener search context
- Optional SSE HTTP service with Privy-backed delegated Solana signing
- Phoenix/Rise perps context with paper-first safety gates

The default feature set is Solana only. HTTP service support is available with
`--features full`. The supported docs and agent surfaces use SVM terminology.

## Quick Start

From this repository:

```bash
cd Kit
cargo check
cargo run --example simple
```

For a service build:

```bash
cargo run --features full --bin kit
```

The service listens on `0.0.0.0:6969` and exposes:

```text
POST /stream
GET  /auth
GET  /healthz
```

## Basic Agent Usage

```rust
use std::sync::Arc;

use openclawd_solana_kit::signer::solana::LocalSolanaSigner;
use openclawd_solana_kit::signer::SignerContext;
use openclawd_solana_kit::solana::agent::create_solana_agent;
use rig::completion::Prompt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let private_key = std::env::var("SOLANA_PRIVATE_KEY")?;
    let signer = LocalSolanaSigner::new(private_key);

    SignerContext::with_signer(Arc::new(signer), async {
        let agent = create_solana_agent(None).await?;
        let response = agent.prompt("what is my public key?").await?;
        println!("{response}");
        Ok(())
    })
    .await
}
```

## Configuration

Local signer mode:

```bash
ANTHROPIC_API_KEY=...
SOLANA_PRIVATE_KEY=...
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
```

HTTP/Privy mode:

```bash
ANTHROPIC_API_KEY=...
SOLANA_RPC_URL=...
PRIVY_APP_ID=...
PRIVY_APP_SECRET=...
PRIVY_VERIFICATION_KEY="-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
```

## Documentation

The mdBook source lives in [`docs`](./docs). Start with:

- [Introduction](./docs/introduction.md)
- [Quick Start](./docs/quickstart.md)
- [Solana SVM tools](./docs/solana.md)
- [Phoenix/Rise perps](./docs/perps.md)
- [HTTP service](./docs/http_service.md)

## License

MIT
