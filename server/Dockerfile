FROM rust:1-slim-bookworm AS chef
RUN cargo install cargo-chef --locked --version ^0.1
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN cargo build --release --locked --bin hamlet

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
        libsqlite3-0 ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1000 hamlet
WORKDIR /app
COPY --from=builder /app/target/release/hamlet /usr/local/bin/hamlet
RUN mkdir -p /app/uploads && chown -R hamlet:hamlet /app
USER hamlet
ENV HAMLET_BIND_ADDR=0.0.0.0:3030
EXPOSE 3030
CMD ["hamlet"]
