# syntax=docker/dockerfile:1
FROM rust:1.86.0-bookworm AS builder
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    --mount=type=ssh,id=default \
    cargo build --release && \
    cp target/release/omni-relayer /tmp/


FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /tmp/omni-relayer /usr/local/bin/
RUN useradd -m -u 1001 appuser
USER appuser
WORKDIR /app
CMD ["omni-relayer"]