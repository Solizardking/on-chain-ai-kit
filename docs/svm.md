# SVM Runtime

OpenClawd Kit is built around the Solana Virtual Machine. The public docs should
describe Solana accounts, programs, token mints, associated token accounts,
Jupiter routes, Pump.fun flows, and Phoenix/Rise perps as SVM-native surfaces.

## Runtime Model

- Accounts hold state.
- Programs own and mutate account data through instructions.
- Transactions bundle instructions and signatures.
- Signers are scoped through `SignerContext`.
- Agents can inspect, prepare, and request approval for SVM actions.

## Naming Rule

Use **SVM** for runtime-level language and **Solana** for the live network,
RPC, wallet, and token context. Keep the OpenClawd docs centered on that
runtime model.

## Perps Boundary

Phoenix/Rise perps are documented as an SVM market extension. Read-only market
and open-interest lookups may be agent-callable. Any order path remains
paper-first until preflight and explicit user approval pass.
