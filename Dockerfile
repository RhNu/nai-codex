# multi-stage build: web -> rust -> runtime

FROM node:20-bullseye AS web
WORKDIR /app/web
COPY web/package.json web/pnpm-lock.yaml ./
COPY web/ .
RUN corepack enable && pnpm install --frozen-lockfile && pnpm run build

FROM rust:1.82-bullseye AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev build-essential && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY libs ./libs
COPY src ./src
# 不需要COPY web/dist，因为下面会从web stage复制
COPY --from=web /app/web/dist ./web/dist
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/codex /app/codex
COPY --from=web /app/web/dist/spa /app/static
VOLUME ["/data"]
ENV CODEX_DB_PATH=/data/codex.redb \
    CODEX_PREVIEW_DIR=/data/previews \
    CODEX_GALLERY_DIR=/data/gallery \
    CODEX_STATIC_DIR=/app/static \
    CODEX_ADDR=0.0.0.0:8080
EXPOSE 8080
CMD ["/app/codex"]
