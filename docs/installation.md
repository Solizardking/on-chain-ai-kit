# Installation

## One-shot (npm / curl)

From a clone of this repository:

```bash
npm install
npm run setup    # copies .env.example → .env if missing
npm run doctor   # Rust + PRIVY_* readiness
npm run kit      # cargo run --features full --bin kit
```

Curl installer (clones or uses local tree, builds `kit`, installs `~/.local/bin/openclawd-kit`):

```bash
curl -fsSL https://raw.githubusercontent.com/clawdsolana/OpenClawd/main/scripts/install.sh | bash
# or: sh scripts/install.sh
```

CLI binary names: `openclawd-kit` / `openclawd-solana-kit` (see root `package.json`).

## Rust (Cargo)

From this repository root (crate lives at the repo root, not a nested `Kit/` folder):

```bash
cargo check
```

The default feature set builds the Solana library. To include the HTTP service:

```bash
cargo check --features full
cargo run --features full --bin kit
```

HTTP requires Privy env vars. The kit loads `.env`, `.env.local`, and `src/.env.local`
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
