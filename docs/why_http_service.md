# Why An HTTP Service?

Local agents are useful for development. A production OpenClawd app needs the
same SVM tools behind a web or TUI workflow where many users can stream agent
responses at once.

The HTTP service provides:

- Server-Sent Events for incremental model and tool output
- Privy JWT authentication
- Per-request Solana signer scoping
- A simple health endpoint for deployment checks

Run it with:

```bash
cp .env.example .env
cargo run --features full --bin kit
```

Set `PRIVY_VERIFICATION_KEY` as a single-line PEM string with escaped newlines:

```bash
PRIVY_VERIFICATION_KEY="-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
```

The next chapter shows the frontend authentication flow.
