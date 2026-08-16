# Multi-stage release image built from the current checkout.
# Usage (from the repository root):
#   docker build -f docker/Dockerfile -t message-vault-server .
# Or:
#   docker compose -f docker/compose.release.yml up --build

# -----------------------------------------------------------------------------
# Stage 1: Vite SPA frontend
# -----------------------------------------------------------------------------
FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 2: Rust binary
# -----------------------------------------------------------------------------
# rusqlite 0.40 / libsqlite3-sys 0.38 need Rust 1.95+ (cfg_select!).
FROM rust:1.95-bookworm AS rust-builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Required at compile time by crates/vault/server/src/db/schema.rs include_str!(…).
COPY schema ./schema

RUN cargo build --release -p message-vault-server
# Conversation JSONL is not in git. Write it here so the runtime image can seed.
RUN cargo run --release -p demo-seed

# -----------------------------------------------------------------------------
# Stage 3: slim runtime
# -----------------------------------------------------------------------------
FROM node:20-bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    ffmpeg \
    tini \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Rust CLI / serve binary
COPY --from=rust-builder /src/target/release/message-vault-server /usr/local/bin/message-vault-server

# Demo bundle (seeded on first start when DEMO_DATA is truthy)
COPY --from=rust-builder /src/crates/vault/demo-seed/staging ./crates/vault/demo-seed/staging
COPY --from=rust-builder /src/crates/vault/demo-seed/config ./crates/vault/demo-seed/config
COPY --from=rust-builder /src/crates/vault/demo-seed/README.md ./crates/vault/demo-seed/README.md

# Container config template
COPY config/config.docker.toml ./config/config.docker.toml

# Vite SPA (built in Stage 1)
COPY --from=frontend-builder /web/dist ./static

COPY docker/entrypoint-release.sh /usr/local/bin/docker-entrypoint-release.sh
RUN chmod +x /usr/local/bin/docker-entrypoint-release.sh \
  && mkdir -p /app/data \
  && chown -R node:node /app

USER node

ENV HOSTNAME=0.0.0.0 \
    VAULT_DB=/app/data/vault.db \
    VAULT_DATA_DIR=/app/data

EXPOSE 8080
VOLUME ["/app/data"]

ENTRYPOINT ["tini", "--", "/usr/local/bin/docker-entrypoint-release.sh"]
