# HTTP Service

The `http` feature exposes a small Server-Sent Events service for SVM agent
sessions.

```bash
# default: local mode (SOLANA_PRIVATE_KEY, no Privy)
cargo run --features full --bin kit
# or: npm run kit
```

The server binds to `0.0.0.0:6969`.

Default auth is **local** (`KIT_AUTH_MODE=local`): `/stream` needs **no** Bearer
token and signs with `SOLANA_PRIVATE_KEY`. Optional: `KIT_AUTH_MODE=privy`.

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

## Frontend

With the kit running (`npm run kit` or `cargo run --features full --bin kit`):

| URL | What |
|-----|------|
| http://localhost:6969/ | Static **Agent Studio** (`frontend/index.html`) — catalog, create, mint UI |
| http://localhost:6969/chat.html | **Stream chat** — paste Privy access token, `POST /stream` SSE |
| http://localhost:6969/healthz | Liveness JSON |
| http://localhost:6969/api/agents/* | Agent catalog (no auth) |

CORS is permissive, so a separate Vite/Next app on another port can call the kit.

### Connect any frontend (local mode — default)

```js
await fetch("http://localhost:6969/stream", {
  method: "POST",
  headers: { "Content-Type": "application/json", Accept: "text/event-stream" },
  body: JSON.stringify({ prompt: "what is my public key?", chat_history: [], chain: "solana" }),
});
// Read body as SSE: data: {"type":"Message"|"ToolCall"|"Error",...}
// Use fetch + ReadableStream (not EventSource — POST required).
```

No login. See `frontend/chat.html`. Optional Privy multi-user: [Authentication](./authentication.md).

## Smoke Test

```bash
./scripts/send-test-req.sh
# expect unauthorized without Bearer token
curl -sS http://127.0.0.1:6969/healthz
```
