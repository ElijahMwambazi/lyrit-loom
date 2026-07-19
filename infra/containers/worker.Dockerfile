FROM rust:1.88-bookworm AS builder

ENV RUSTUP_TOOLCHAIN=1.88.0 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=10
WORKDIR /app
COPY . .
RUN --mount=type=cache,id=lyrit-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=lyrit-cargo-git,target=/usr/local/cargo/git,sharing=shared \
    --mount=type=cache,id=lyrit-worker-target,target=/app/target,sharing=shared \
    cargo build --locked --release --bin lyrit-worker \
    && cp /app/target/release/lyrit-worker /tmp/lyrit-worker

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates ffmpeg python3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 lyrit \
    && mkdir --parents /app/workspaces /app/artifacts \
    && chown --recursive lyrit:lyrit /app

WORKDIR /app
COPY --from=builder /tmp/lyrit-worker /usr/local/bin/lyrit-worker
COPY apps/transcriber /app/apps/transcriber
COPY contracts /app/contracts
USER lyrit
ENTRYPOINT ["/usr/local/bin/lyrit-worker"]
