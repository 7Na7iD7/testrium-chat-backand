FROM rust:1.82-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/chat-domain/Cargo.toml crates/chat-domain/Cargo.toml
COPY crates/chat-infra/Cargo.toml crates/chat-infra/Cargo.toml
COPY crates/chat-server/Cargo.toml crates/chat-server/Cargo.toml

RUN mkdir -p crates/chat-domain/src crates/chat-infra/src crates/chat-server/src \
    && echo "fn main() {}" > crates/chat-server/src/main.rs \
    && echo "" > crates/chat-domain/src/lib.rs \
    && echo "" > crates/chat-infra/src/lib.rs \
    && cargo build --release --package chat-server \
    && rm -rf crates/chat-domain/src crates/chat-infra/src crates/chat-server/src

COPY crates ./crates
COPY migrations ./migrations

RUN touch crates/chat-domain/src/lib.rs crates/chat-infra/src/lib.rs crates/chat-server/src/main.rs \
    && cargo build --release --package chat-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/chat-server /app/chat-server
COPY --from=builder /app/migrations /app/migrations

EXPOSE 9090

CMD ["/app/chat-server"]
