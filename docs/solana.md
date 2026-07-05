# Solana SVM

The Solana module is the default OpenClawd SVM Kit surface. It exposes
agent-callable tools for portfolio context, token actions, swaps, Pump.fun
flows, and SVM-native market context.

## Tools

```text
perform_jupiter_swap(input_mint, input_amount, output_mint, slippage_bps)
transfer_sol(to, amount)
transfer_spl_token(to, amount, mint)
get_public_key()
get_sol_balance()
get_spl_token_balance(mint)
fetch_token_price(mint)
get_portfolio()
search_on_dex_screener(query)
deploy_pump_fun_token(...)
buy_pump_fun_token(mint, sol_amount, slippage_bps)
sell_pump_fun_token(mint, token_amount)
```

Perps live in the Phoenix/Rise extension surface rather than the base token
tools. Keep them paper-first:

```text
perps_market(symbol)
perps_open_interest(symbol)
perps_paper_order(side, symbol, notional_usdc)
perps_preflight()
```

## Amounts

- SOL transfers use lamports.
- SPL token amounts are raw integer units, already adjusted for decimals.
- Slippage is in basis points.
- Perps notional values are denominated in USDC unless a market adapter states
  otherwise.

When decimals are unknown, call `get_spl_token_balance` or fetch token metadata
before preparing a transfer.

## Safety

Transaction-producing tools do not sign directly. They create transactions
inside `execute_solana_transaction`, which pulls the current signer from
`SignerContext`. If no signer has been bound to the async scope, the call fails
instead of falling back to a global key.

Phoenix/Rise perps must follow the same rule. Paper mode is the default, and
live order placement requires an explicit approval gate after preflight.

## RPC

Set `SOLANA_RPC_URL` for production use. The public mainnet endpoint is a
development fallback and should not be used for high-volume agent workflows.
