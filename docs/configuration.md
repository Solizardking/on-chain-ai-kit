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

## Auth modes (`KIT_AUTH_MODE`)

| Mode | Default | Needs | `/stream` auth |
|------|---------|-------|----------------|
| **`local`** | **yes** | `SOLANA_PRIVATE_KEY`, `ANTHROPIC_API_KEY` | **None** (open) |
| `privy` | no | `PRIVY_*` + Anthropic | Bearer Privy JWT |

```bash
KIT_AUTH_MODE=local   # default — no Privy
```

Local mode is for **dev only**. Anyone who can hit the port can spend from your key.

## Required For Agents / local HTTP

```bash
ANTHROPIC_API_KEY=...
SOLANA_PRIVATE_KEY=...
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
```

The bundled agent builder uses Anthropic through `rig-core`.

```bash
npm run kit
# or: cargo run --features full --bin kit
```

## Optional: HTTP with Privy (multi-user)

```bash
KIT_AUTH_MODE=privy
PRIVY_APP_ID=...
PRIVY_APP_SECRET=...
PRIVY_VERIFICATION_KEY="-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
SOLANA_RPC_URL=...
```

See [Authentication](./authentication.md).

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
