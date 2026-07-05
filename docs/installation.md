# Installation

From this repository:

```bash
cd Kit
cargo check
```

The default feature set builds the Solana library:

```bash
cargo check
```

To include the HTTP service:

```bash
cargo check --features full
```

To run the service:

```bash
cargo run --features full --bin kit
```

If you are embedding the crate from a sibling project, depend on it by path:

```toml
[dependencies]
openclawd-solana-kit = { path = "../Kit", features = ["solana"] }
```

Custom tools use the same macro system as the built-in tools:

```bash
cargo add rig-tool-macro
```

On minimal Linux images, install TLS libraries before building:

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates openssl libssl3
```
