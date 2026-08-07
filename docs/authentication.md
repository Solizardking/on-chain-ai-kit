# Authentication

## Default: local mode (no Privy)

`KIT_AUTH_MODE=local` (default). The kit signs with `SOLANA_PRIVATE_KEY`.
**No login, no Bearer header.**

```bash
# open stream — no Authorization
curl -N -X POST http://localhost:6969/stream \
  -H "Content-Type: application/json" \
  -d '{"prompt":"what is my public key?","chat_history":[],"chain":"solana"}'
```

Frontend (`frontend/chat.html`): open http://localhost:6969/chat.html and send.

`GET /auth` returns the local pubkey. `GET /healthz` includes `"auth_mode":"local"`.

**Do not expose a local-mode kit on the public internet.**

## Optional: Privy mode

```bash
KIT_AUTH_MODE=privy
PRIVY_APP_ID=...
PRIVY_APP_SECRET=...
PRIVY_VERIFICATION_KEY="-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
```

The service expects a Privy access token in the `Authorization` header.
The frontend owns login; the Rust service verifies the token and builds a
`PrivySigner` for the request.

```ts
import { usePrivy } from "@privy-io/react-auth";

async function sendMessage(prompt: string) {
  const { getAccessToken } = usePrivy();
  const token = await getAccessToken();

  return fetch("http://localhost:6969/stream", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      prompt,
      chat_history: [],
      chain: "solana",
    }),
  });
}
```

The middleware validates the JWT; tool execution is scoped to that user's
delegated Solana wallet.
