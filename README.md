# Lyrit Loom

Weave words into motion.

Private monorepo for turning audio, a background image, and editable word timestamps into polished lyric videos.

Milestones 0 and 1 are executable: projects and validated source media are durable, while the separate PostgreSQL-backed worker and deterministic fake transcriber preserve the foundation for transcription and rendering.

## Run it

Requirements: Docker with the Compose plugin.

```bash
docker compose up --build -d
```

Open [http://localhost:3000](http://localhost:3000), create a project, then choose or drop an audio file and background image into its source cards. Upload progress is visible in the browser; the API streams, hashes, probes, stores, and activates supported media. Once audio is ready, **Transcribe audio** runs normalization and the deterministic fake transcriber in the durable worker, then displays the word-timed result. The Milestone 0 queue check remains available under **Foundation diagnostics**.

From another terminal, the same check is available as:

```bash
make smoke
```

The verified readiness endpoint is `http://localhost:8080/api/v1/health/ready`.

Stop and remove the application containers with:

```bash
make down
```

The PostgreSQL volume is retained. Remove it only when you intentionally want to discard local project/job data.

## What exists now

- Axum API with project CRUD, streamed multipart source uploads, liveness/readiness, job lookup, SSE event stream, request IDs, and a dev-only probe endpoint.
- Atomic local artifact storage with generated keys, SHA-256 checksums, upload limits, and bounded ffprobe validation for audio and background images.
- Separate Tokio worker claiming PostgreSQL jobs with row locks, leases, heartbeats, monotonic progress, and terminal events.
- SQLx migrations for projects/assets, durable jobs, and event history.
- React 19 + Vite 8 project workspace with drag/drop upload progress using current OpenAPI-generated types.
- Versioned fake transcriber CLI and JSON Schema contract; no Whisper model download required yet.
- Compose images, Nginx same-origin API proxy, CI, lockfile workflows, scripts, architecture documents, and an accepted ADR.

## Repository map

```text
apps/
  api/             Axum composition root and HTTP routes
  worker/          durable job runner
  web/             React/Vite control surface
  transcriber/     Python process adapter (fake in Milestone 0)
crates/
  domain/          framework-free job entities/state
  application/     repository port and job use cases
  persistence/     SQLx PostgreSQL adapter
  media/           local artifact storage and ffprobe adapter
  api-model/       serialized HTTP response models
contracts/
  openapi.yaml
  transcriber.schema.json
db/migrations/     forward-only schema changes
infra/             container and Nginx configuration
packages/api-client/
docs/              architecture, delivery plan, development guide, ADRs
scripts/           toolchain, contract generation, tests, smoke probe
```

## Develop on the host

```bash
cp .env.example .env
docker compose up -d postgres
make install
make generate-api
make check
```

Run these in separate terminals:

```bash
cargo run -p lyrit-api
cargo run -p lyrit-worker
pnpm dev:web
```

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for contract workflow, fake transcriber use, test commands, and production guardrails.

## Architecture and delivery

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — boundaries, component ownership, data model, state machines, and API/process contracts.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — completed work, the current focus, and upcoming product milestones.
- [`docs/DELIVERY_GUIDE.md`](docs/DELIVERY_GUIDE.md) — milestones and implementation notes for Rust, React, Whisper, ASS, and FFmpeg.
- [`contracts/openapi.yaml`](contracts/openapi.yaml) — OpenAPI 3.1 source of truth.

Milestone 2 provides the complete model-free transcription and review path. Milestone 3 now includes a synchronized waveform, accessible transport and word text/timing controls, plus immutable saves protected by optimistic concurrency; structural cue editing comes next.

This repository is proprietary and is not licensed for public distribution.
