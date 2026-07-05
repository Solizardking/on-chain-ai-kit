# Authentication

The HTTP service expects a Privy access token in the `Authorization` header.
The frontend owns login; the Rust service verifies the token and builds a
`PrivySigner` for the request.

Backend environment:

```bash
PRIVY_APP_ID=...
PRIVY_APP_SECRET=...
PRIVY_VERIFICATION_KEY="-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
```

Frontend request shape:

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

The middleware validates the JWT audience against `PRIVY_APP_ID`. The route then
fetches the user session and scopes tool execution to that user's delegated
Solana wallet.
