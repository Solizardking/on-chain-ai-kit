# Phoenix/Rise Perps

OpenClawd perps support is SVM-native and paper-first. The kit should expose
Phoenix/Rise market context to agents without giving them an automatic live
execution path.

## Default Flow

```text
health -> market -> open-interest signal -> risk check -> paper order
```

Live order placement is outside the default flow. It requires:

- explicit user approval
- a passing preflight
- concrete `side`, `symbol`, and `notional_usdc`
- signer context scoped to the current request

## Agent Surface

```text
perps_health()
perps_market(symbol)
perps_open_interest(symbol)
perps_paper_order(side, symbol, notional_usdc)
perps_preflight()
```

These names document the adapter contract. Implementations can call a local
Phoenix/Rise service, a Python perps agent, or an OpenClawd perps module as long
as the safety contract stays the same.

## OODA Integration

The OODA loop can consume perps open-interest signals as observation context.
The signal can influence a paper decision, but it does not bypass risk limits,
loss kill-switches, or approval gates.

## Safety Contract

- Paper mode is default.
- Live mode must be selected explicitly.
- Live orders require approval for each action.
- Preflight checks must run before live mode.
- Private keys, wallet passwords, and raw secrets must never be printed.
