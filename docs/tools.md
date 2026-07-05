# Tools

OpenClawd SVM Kit tools are regular async Rust functions annotated with
`#[tool]` from `rig-tool-macro`.

```rust
use rig_tool_macro::tool;

#[tool]
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

Attach a generated tool type to an agent:

```rust
let agent = rig::providers::anthropic::Client::from_env()
    .agent(rig::providers::anthropic::CLAUDE_3_5_SONNET)
    .preamble("you are an SVM portfolio assistant")
    .max_tokens(1024)
    .tool(GetPortfolio)
    .build();
```

## Transaction Tools

Transaction tools should follow this shape:

```rust
#[tool]
pub async fn transfer_sol(to: String, amount: u64) -> anyhow::Result<String> {
    execute_solana_transaction(move |owner| async move {
        create_transfer_sol_tx(&Pubkey::from_str(&to)?, amount, &owner).await
    })
    .await
}
```

The helper gets the current signer from `SignerContext`, builds a transaction,
and hands it to the configured signer implementation.

## Tool Inputs

The tool macro works best with JSON-native parameters:

- `String`
- `bool`
- integer and floating-point numbers

Parse Solana `Pubkey` values inside the tool body so invalid user input returns
a normal tool error.
