# ADR 0001: Modular monolith with a durable worker

- **Status:** accepted
- **Date:** 2026-07-16

## Context

Transcription and video encoding are long-running and resource-heavy, while project and editor APIs must remain responsive. The first release does not justify independent network services, a broker, or Kubernetes.

## Decision

Use one Rust workspace with separate `api` and `worker` binaries. Share domain/application crates, persist job state and events in PostgreSQL, and claim jobs with short row-locking transactions plus leases. Keep Whisper and FFmpeg behind process adapters.

## Consequences

- The HTTP process never performs transcription or rendering.
- PostgreSQL is sufficient infrastructure for the first release.
- Jobs remain observable and recoverable across process restarts.
- Transcription/render workers can later split onto specialized hosts without changing the application contract.
- Queue lease, retry, and idempotency behavior require dedicated integration tests.
