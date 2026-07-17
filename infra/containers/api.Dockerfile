FROM rust:1.88-bookworm AS builder

ENV RUSTUP_TOOLCHAIN=1.88.0
WORKDIR /app
COPY . .
RUN cargo build --release --bin lyrit-api

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 lyrit

COPY --from=builder /app/target/release/lyrit-api /usr/local/bin/lyrit-api
USER lyrit
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/lyrit-api"]
