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

EXPOSE 6969

FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/kit /usr/local/bin/openclawd-kit
ENTRYPOINT ["/usr/local/bin/openclawd-kit"]
