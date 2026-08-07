# Installation

## Hosted one-shot (Fly)

No install required — open Agent Studio:

| Surface | URL |
|---------|-----|
| Agent Studio | https://openclawd-solana-kit.fly.dev/ |
| Stream chat | https://openclawd-solana-kit.fly.dev/chat.html |
| Health | https://openclawd-solana-kit.fly.dev/healthz |

Maintainers deploy with `fly deploy` (see root `fly.toml` + `Dockerfile`).

## One-shot (npm / curl)

From a clone of this repository:

```bash
npm install
npm run setup    # copies .env.example → .env if missing
# set SOLANA_PRIVATE_KEY=...  (XAI_API_KEY optional for Grok 4.5)
npm run doctor
npm start        # cargo run --features full --bin kit
# open http://127.0.0.1:6969/
```

npx from GitHub:

```bash
npx --yes github:Solizardking/on-chain-ai-kit doctor
npx --yes github:Solizardking/on-chain-ai-kit start
```

Curl installer (clones or uses local tree, builds `kit`, installs `~/.local/bin/openclawd-kit`):

```bash
curl -fsSL https://raw.githubusercontent.com/Solizardking/on-chain-ai-kit/main/scripts/install.sh | bash
# or: sh scripts/install.sh
```

CLI binary names: `openclawd-kit` / `openclawd-solana-kit` (see root `package.json`).

## Rust (Cargo)

From this repository root (crate lives at the repo root):

```bash
cargo check
```

The default feature set builds the Solana library. To include the HTTP service:

```bash
cargo check --features full
cargo run --features full --bin kit
```

HTTP defaults to **local** auth (`SOLANA_PRIVATE_KEY`). Optional Privy multi-user mode:
`KIT_AUTH_MODE=privy` + `PRIVY_*`. The kit loads `.env`, `.env.local`, and `src/.env.local`
automatically (see [Configuration](./configuration.md)).

If you are embedding the crate from a sibling project, depend on it by path:

```toml
[dependencies]
openclawd-solana-kit = { path = "../on-chain-ai-kit", features = ["solana"] }
```

Custom tools use the same macro system as the built-in tools:

```bash
cargo add rig-tool-macro
```

On minimal Linux images, install TLS libraries before building:

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates openssl libssl3
```
