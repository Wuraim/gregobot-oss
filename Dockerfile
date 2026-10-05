# syntax=docker/dockerfile:1.4
ARG RUST_VERSION=1.97
ARG DEBIAN_VERSION=bookworm

# ── Stage 1: builder ──────────────────────────────────────────────────────────
FROM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} AS builder
WORKDIR /build
ENV CARGO_TERM_COLOR=always

# Build-time deps for openssl-sys / libsqlite3-sys (headers + pkg-config).
# These are NOT needed at runtime; they only affect the builder stage.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Source first — no dummy-main trick needed with BuildKit cache mounts.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY .sqlx ./.sqlx
COPY assets ./assets

# Two cache mounts:
#   - /usr/local/cargo/registry  → crates.io download cache
#   - /build/target              → incremental compilation cache
# Both persist across builds via BuildKit's cache backend (no layer cost).
# We MUST copy the binary out within this same RUN, since the cache mount
# contents don't end up in the output layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/build/target,sharing=locked \
    cargo build --release --locked \
    && cp target/release/gregobot /usr/local/bin/gregobot

# ── Stage 2: runtime ──────────────────────────────────────────────────────────
FROM debian:${DEBIAN_VERSION}-slim AS runtime

ARG APP_VERSION=unknown
ARG GIT_SHA=unknown
ARG BUILD_DATE=unknown
LABEL org.opencontainers.image.source="https://github.com/Wuraim/gregobot-oss" \
      org.opencontainers.image.description="Personal Telegram bot + REST admin API" \
      org.opencontainers.image.version="${APP_VERSION}" \
      org.opencontainers.image.revision="${GIT_SHA}" \
      org.opencontainers.image.created="${BUILD_DATE}"

# ca-certificates: outbound TLS (Telegram / Gemini / Google).
# libssl3 / libsqlite3-0: runtime shared libs the binary links against.
# curl: only for the HEALTHCHECK below.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        libsqlite3-0 \
        curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/gregobot /usr/local/bin/gregobot

# Bake the persona file in — previously was being silently dropped by .dockerignore.
COPY prompt /prompt

RUN mkdir /data
WORKDIR /data
EXPOSE 8080

# /health returns 200 {"status":"ok"} — defined in src/api/mod.rs.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/gregobot"]
