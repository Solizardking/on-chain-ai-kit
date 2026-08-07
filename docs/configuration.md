# Configuration

OpenClawd SVM Kit reads environment variables with multi-path `dotenv` support.

## Env file locations

Loaded in order (process env always wins; files fill missing keys only):

1. `CLAWD_ENV_FILE` if set (absolute path to one file)
2. `.env`, `.env.local` under the process cwd
3. `src/.env.local` under the process cwd
4. Same names under the crate root (`CARGO_MANIFEST_DIR`)

Template: copy [`.env.example`](../.env.example) to `.env` or `src/.env.local`.

```bash
cp .env.example .env
# or keep secrets in src/.env.local (gitignored)
npm run doctor   # verifies PRIVY_* and optional agent keys
```

If `cargo run --features full --bin kit` prints `MissingEnvVar("PRIVY_APP_ID")`,
no loaded file contained that key. Fix with `npm run setup` + edit, or export vars.

## Required For Agents

```bash
ANTHROPIC_API_KEY=...
```

The bundled agent builder uses Anthropic through `rig-core`.

## Local Solana Signing

```bash
SOLANA_PRIVATE_KEY=...
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
```

`SOLANA_PRIVATE_KEY` is only needed when using `LocalSolanaSigner`. The RPC URL
defaults to the public mainnet endpoint when unset.

## HTTP Service With Privy

```bash
PRIVY_APP_ID=...
PRIVY_APP_SECRET=...
PRIVY_VERIFICATION_KEY="-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
SOLANA_RPC_URL=...
```

When the `http` feature is enabled, user signing is delegated through Privy.
Do not pass local private keys to the HTTP service.

```bash
npm run kit
# equivalent: cargo run --features full --bin kit
```

## Phoenix/Rise Perps

Perps integrations should start in paper mode:

```bash
PERPS_MODE=paper
PERPS_MARKET=SOL-PERP
PHOENIX_PERPS_URL=...
RISE_PERPS_URL=...
```

Live order execution must require explicit user approval, a passing preflight,
and a concrete market/side/notional payload. The kit should never read or print
private keys while resolving Phoenix/Rise market context.
