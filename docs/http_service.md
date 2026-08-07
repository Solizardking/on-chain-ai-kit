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

## Frontend

With the kit running (`npm run kit` or `cargo run --features full --bin kit`):

| URL | What |
|-----|------|
| http://localhost:6969/ | Static **Agent Studio** (`frontend/index.html`) — catalog, create, mint UI |
| http://localhost:6969/chat.html | **Stream chat** — paste Privy access token, `POST /stream` SSE |
| http://localhost:6969/healthz | Liveness JSON |
| http://localhost:6969/api/agents/* | Agent catalog (no auth) |

CORS is permissive, so a separate Vite/Next app on another port can call the kit.

### Separate React app (Privy)

1. Backend: same `PRIVY_APP_ID` / secret / verification key as in `src/.env.local`.
2. Frontend: `@privy-io/react-auth` with that **public** `appId`.
3. After `login()`, `const token = await getAccessToken()`.
4. `POST http://localhost:6969/stream` with header `Authorization: Bearer ${token}` and body `{ prompt, chat_history: [], chain: "solana" }`.
5. Read the response as **SSE** (`data: {"type":"Message"|"ToolCall"|"Error",...}`). Use `fetch` + `ReadableStream` (not `EventSource`, which is GET-only).

See [Authentication](./authentication.md) and `frontend/chat.html` for a working stream client.

## Smoke Test

```bash
./scripts/send-test-req.sh
# expect unauthorized without Bearer token
curl -sS http://127.0.0.1:6969/healthz
```
