# Configuration

OpenClawd SVM Kit reads environment variables with `dotenv` support.

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
