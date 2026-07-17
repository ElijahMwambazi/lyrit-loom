FROM rust:1.88-bookworm AS builder

ENV RUSTUP_TOOLCHAIN=1.88.0
WORKDIR /app
COPY . .
RUN cargo build --release --bin lyrit-worker

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates ffmpeg python3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 lyrit \
    && mkdir --parents /app/workspaces /app/artifacts \
    && chown --recursive lyrit:lyrit /app

WORKDIR /app
COPY --from=builder /app/target/release/lyrit-worker /usr/local/bin/lyrit-worker
COPY apps/transcriber /app/apps/transcriber
COPY contracts /app/contracts
USER lyrit
ENTRYPOINT ["/usr/local/bin/lyrit-worker"]
