# OpenClawd Solana Kit

**SVM-first Rust agents for Solana** — `rig-core` tools, `SignerContext` signing, optional Privy SSE HTTP service, Clawd constitution (Law I–III).

Crate: `openclawd-solana-kit` · Binary: `kit` · CLI: `openclawd-kit`

```text
default feature = solana
full             = solana + http (Privy SSE on 0.0.0.0:6969)
```

---

## One-shot install (npm / curl)

### Hosted (Fly — open Agent Studio now)

| Surface | URL |
|---------|-----|
| **Agent Studio** | https://openclawd-solana-kit.fly.dev/ |
| **Stream chat** | https://openclawd-solana-kit.fly.dev/chat.html |
| **Health** | https://openclawd-solana-kit.fly.dev/healthz |

```bash
# maintainers: redeploy from clone
fly deploy --ha=false
# fly secrets set SOLANA_PRIVATE_KEY=... XAI_API_KEY=...
```

### Local one-shot (npm)

```bash
cd on-chain-ai-kit
npm install
npm run setup          # copies .env.example → .env if needed
# edit .env: SOLANA_PRIVATE_KEY=...  (XAI_API_KEY optional → Grok 4.5)
npm run doctor
npm start              # Agent Studio + API on :6969
# open http://127.0.0.1:6969/  and  /chat.html
```

### npx (from git / after publish)

```bash
npx --yes github:Solizardking/on-chain-ai-kit doctor
npx --yes github:Solizardking/on-chain-ai-kit start
# after npm publish:  npx openclawd-solana-kit start
```

### curl | bash

```bash
curl -fsSL https://raw.githubusercontent.com/Solizardking/on-chain-ai-kit/main/scripts/install.sh | bash
export PATH="$HOME/.local/bin:$PATH"
openclawd-kit doctor
openclawd-kit start
```

Local clone: `sh scripts/install.sh`

---

## HTTP kit (local + mesh / xAI)

```bash
# needs SOLANA_PRIVATE_KEY in .env or src/.env.local
# LLM: XAI_API_KEY → Grok 4.5; else Clawd free mesh
cargo run --features full --bin kit
# or: npm start
```

| Concern | Default |
|---------|---------|
| Auth | **`local`** — `SOLANA_PRIVATE_KEY`, no Bearer |
| LLM (preferred) | **`XAI_API_KEY`** → Grok **4.5** · `https://api.x.ai/v1/responses` |
| LLM (fallback) | Clawd mesh · `zkrouter/auto` · `https://clawd-inference-mesh.fly.dev/v1` |
| Frontend | `/` Agent Studio · `/chat.html` · same-origin `/stream` |
| Optional multi-user | `KIT_AUTH_MODE=privy` + `PRIVY_*` |

```bash
open http://127.0.0.1:6969/
open http://127.0.0.1:6969/chat.html
curl -s http://127.0.0.1:6969/healthz
# → auth_mode, llm_provider, mesh_model, …
```

Env: [`.env.example`](./.env.example) · [docs/configuration.md](./docs/configuration.md)

---

## Quick start (library / examples)

```bash
# Rust only
cargo check
cargo run --example simple
cargo run --example solana_agent
```

Needs `ANTHROPIC_API_KEY` + `SOLANA_PRIVATE_KEY` (+ optional `SOLANA_RPC_URL`).  
Details: [docs/quickstart.md](./docs/quickstart.md) · [docs/installation.md](./docs/installation.md)

### Minimal agent

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
        println!("{}", agent.prompt("what is my public key?").await?);
        Ok(())
    }).await
}
```

---

## HTTP service

```bash
npm run kit
# GET  http://127.0.0.1:6969/healthz
# POST http://127.0.0.1:6969/stream   (Authorization: Bearer <Privy access token>)
# GET  http://127.0.0.1:6969/auth
# GET  http://127.0.0.1:6969/api/agents/*
```

Do **not** put `SOLANA_PRIVATE_KEY` on multi-user HTTP — signing is Privy-delegated ([docs/http_service.md](./docs/http_service.md), [docs/why_http_service.md](./docs/why_http_service.md)).

Smoke (expect unauthorized without JWT):

```bash
./scripts/send-test-req.sh
```

---

## CLI reference (`openclawd-kit`)

| Command | Action |
|---------|--------|
| `setup` | Copy `.env.example` → `.env` |
| `doctor` | Rust + env readiness |
| `check` | `cargo check` |
| `build` | `cargo build --features full --bin kit` |
| `start` / `npm run kit` | Run HTTP service |
| `example simple` | Portfolio demo |
| `example solana_agent` | Full tool agent + reasoning loop |

---

## What the crate includes

| Surface | Notes |
|---------|--------|
| Solana tools | Jupiter, transfers, balances, portfolio, price, Pump.fun, DexScreener |
| PumpSwap tools | Implemented as `#[tool]`; not yet attached to default agent |
| `SignerContext` | Per-request / per-task signer isolation |
| `constitution` | Law I–III + CLAWD rules in agent preambles |
| HTTP (`full`) | SSE stream, Privy auth, agent catalog, static `frontend/` |
| EVM / LiFi | Feature-gated legacy; not the supported path today |
| Perps | Documented paper-first; no `perps_*` tools in `src/` yet |

---

## Repo map

```text
src/                 Rust crate (solana, signer, http, constitution, …)
examples/            simple · solana_agent
docs/                mdBook (install, config, tools, HTTP, auth)
npm/bin/             openclawd-kit CLI
scripts/install.sh   curl one-shot installer
ooda/                paper OODA loop + TUI
automaton/           self-running TS agent / Crustacean stack
zk-primitives/       nullifiers · proofs · Light Protocol
lobster-council/     voice council personas
```

### Crustacean Automation

```bash
CLAWD_SKIP_START=1 CLAWD_LOCAL=1 sh scripts/crustacean-automation.sh
```

### OODA (paper only)

```bash
cd ooda && npm install
npm run loop -- --ticks 50 --sleep 0.25
```

---

## Documentation

| Topic | Doc |
|-------|-----|
| Intro / SVM | [introduction](./docs/introduction.md) · [svm](./docs/svm.md) |
| Install / Config / Quickstart | [installation](./docs/installation.md) · [configuration](./docs/configuration.md) · [quickstart](./docs/quickstart.md) |
| Tools / Signer | [tools](./docs/tools.md) · [signer_context](./docs/signer_context.md) · [solana](./docs/solana.md) |
| HTTP / Auth | [http_service](./docs/http_service.md) · [authentication](./docs/authentication.md) · [why](./docs/why_http_service.md) |
| Perps (design) | [perps](./docs/perps.md) |
| Design essays | [clawd-solana-svm-design.md](./clawd-solana-svm-design.md) · [brave-new-world-blockchain-ai.md](./brave-new-world-blockchain-ai.md) |

---

## Safety

- **Never commit** `.env`, `.env.local`, `src/.env.local` (gitignored).
- HTTP = Privy only; examples = local key optional.
- Constitution: Law I (never harm) overrides II (earn) overrides III (no deception).
- OODA is paper/devnet by default — not live trading.

---

## License

MIT
