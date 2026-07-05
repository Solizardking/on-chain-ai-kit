# HTTP Service

The `http` feature exposes a small Server-Sent Events service for SVM agent
sessions.

```bash
cargo run --features full --bin kit
```

The server binds to `0.0.0.0:6969`.

## Endpoints

```text
POST /stream
GET  /auth
GET  /healthz
```

## Stream Request

```ts
{
  prompt: string;
  chat_history: Message[];
  chain?: "solana";
  preamble?: string;
}
```

`chain` defaults to `"solana"` when omitted.

## Stream Events

```ts
type StreamResponse =
  | { type: "Message"; content: string }
  | { type: "ToolCall"; content: { name: string; result: string } }
  | { type: "Error"; content: string };
```

Each request creates a Privy-backed signer from the authenticated user session,
then runs the SVM-aware Solana agent inside `SignerContext`.

## Smoke Test

```bash
./scripts/send-test-req.sh
```
