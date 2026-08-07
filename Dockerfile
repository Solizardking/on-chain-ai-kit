# OpenClawd Solana Kit — multi-stage image for Fly / Docker
# Serves Agent Studio frontend + kit HTTP API on :6969

FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --features full --release --recipe-path recipe.json
COPY . .
RUN cargo build --features full --release --bin kit

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

# Kit binary
COPY --from=builder /app/target/release/kit /usr/local/bin/openclawd-kit
RUN chmod +x /usr/local/bin/openclawd-kit

# Agent Studio + stream chat (same-origin /stream)
COPY frontend /app/frontend

ENV CLAWD_FRONTEND_DIR=/app/frontend
ENV KIT_AUTH_MODE=local
ENV RUST_LOG=info
ENV PORT=6969

EXPOSE 6969

# Fly / Docker health: GET /healthz
HEALTHCHECK --interval=30s --timeout=5s --start-period=40s --retries=3 \
  CMD curl -fsS http://127.0.0.1:6969/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/openclawd-kit"]
