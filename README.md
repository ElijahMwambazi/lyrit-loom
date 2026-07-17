# Lyrit Loom

Weave words into motion.

Private monorepo for turning audio, a background image, and editable word timestamps into polished lyric videos.

Milestone 0 is executable: it boots a Rust API, PostgreSQL-backed durable worker, React/Vite control surface, generated TypeScript API client, and a deterministic fake transcription process boundary.

## Run it

Requirements: Docker with the Compose plugin.

```bash
docker compose up --build -d
```

Open [http://localhost:3000](http://localhost:3000), then select **Run probe job**. The UI queues real durable work, follows persisted progress over Server-Sent Events, and shows the worker result.

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

- Axum API with liveness/readiness, job lookup, SSE event stream, request IDs, and a dev-only probe endpoint.
- Separate Tokio worker claiming PostgreSQL jobs with row locks, leases, heartbeats, monotonic progress, and terminal events.
- SQLx migration for durable jobs and event history.
- React 19 + Vite 8 landing/control surface using an OpenAPI-generated client.
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
- [`docs/DELIVERY_GUIDE.md`](docs/DELIVERY_GUIDE.md) — milestones and implementation notes for Rust, React, Whisper, ASS, and FFmpeg.
- [`contracts/openapi.yaml`](contracts/openapi.yaml) — OpenAPI 3.1 source of truth.

The next vertical slice is Milestone 1: projects, streamed media uploads, local artifact storage, and ffprobe validation.

This repository is proprietary and is not licensed for public distribution.
