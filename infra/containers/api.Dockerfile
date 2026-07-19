FROM rust:1.88-bookworm AS builder

ENV RUSTUP_TOOLCHAIN=1.88.0 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=10
WORKDIR /app
COPY . .
RUN --mount=type=cache,id=lyrit-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=lyrit-cargo-git,target=/usr/local/cargo/git,sharing=shared \
    --mount=type=cache,id=lyrit-api-target,target=/app/target,sharing=shared \
    cargo build --locked --release --bin lyrit-api \
    && cp /app/target/release/lyrit-api /tmp/lyrit-api

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 lyrit \
    && mkdir --parents /app/artifacts \
    && chown --recursive lyrit:lyrit /app

COPY --from=builder /tmp/lyrit-api /usr/local/bin/lyrit-api
WORKDIR /app
USER lyrit
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/lyrit-api"]
