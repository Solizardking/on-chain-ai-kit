# SVM Routing

SVM routing describes how OpenClawd moves between Solana programs, token
accounts, markets, and agent workflows without importing outside-runtime
assumptions.

## Routing Surfaces

- Wallet to token account transfers
- Jupiter-routed token swaps
- Pump.fun launch and trade flows
- Phoenix/Rise perps market context
- Agent-to-agent handoffs through OpenClawd sessions

## Approval Rule

Any route that can move funds, open exposure, or alter a position must go
through an approval gate with a concrete action summary. Read-only SVM routing
can run automatically.
