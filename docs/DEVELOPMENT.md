# Development guide

## Local modes

### Full stack in containers

```bash
docker compose up --build -d
```

Open `http://localhost:3000`. The API is also exposed at `http://localhost:8080/api/v1` for direct debugging. The API applies database migrations before reporting ready; the worker starts after API readiness.

Run the durable job check from a second terminal:

```bash
make smoke
```

### Fast host development

Start only PostgreSQL:

```bash
docker compose up -d postgres
```

Then run each process in its own terminal:

```bash
cargo run -p lyrit-api
cargo run -p lyrit-worker
pnpm dev:web
```

The Vite development server proxies `/api` to port `8080`, so local browser requests remain same-origin from application code.

## First-time setup

```bash
cp .env.example .env
make install
make generate-api
./scripts/check-toolchain.sh
```

`Cargo.lock`, `pnpm-lock.yaml`, the generated TypeScript schema, and `uv.lock` are committed. `make install` enforces the JavaScript and Python lockfiles; do not refresh them incidentally.

## Contract workflow

`contracts/openapi.yaml` is the public HTTP source of truth. After changing it:

```bash
make generate-api
make web-check
```

Frontend code imports `createApiClient` from `@lyrit/api-client`. Do not hand-maintain API response types that already exist in the generated schema.

`contracts/transcriber.schema.json` is the Rust/Python process contract. Milestone 0 implements `LYRIT_TRANSCRIBER_MODE=fake`; it writes deterministic word timestamps atomically and gives worker integration tests a model-free target.

Example transcriber invocation:

```bash
PYTHONPATH=apps/transcriber/src \
  python3 -m lyrit_transcriber --request /absolute/path/request.json
```

The request's `input_path` must exist. Structured output goes only to the request's `output_path`; stdout/stderr are diagnostic logs.

## Durable probe job

`POST /api/v1/internal/dev/jobs/probe` is enabled only when `ENABLE_DEV_ROUTES=true`. It is intentionally absent from the public OpenAPI contract.

The probe demonstrates the real queue mechanics:

1. API inserts a `queued` job and first event in one transaction.
2. Worker claims it with `FOR UPDATE SKIP LOCKED`, records a lease, and commits.
3. Worker persists monotonic phases/events outside the request lifecycle.
4. Browser receives events through SSE and may recover state from `GET /jobs/{id}`.
5. Worker commits the terminal result and releases the lease.

The probe is scaffolding, not a production job type. Milestone 2 replaces it with the transcription handler while preserving the queue path.

## Checks

```bash
make check
```

This runs Rust formatting, Clippy with warnings denied, Rust tests, generated-client drift detection, TypeScript type-checking, React tests, the production web build, Python contract tests, repository JSON/JSON Schema/YAML/TOML/shell validation, Docker Compose configuration validation, and OpenAPI linting.

Or run areas independently:

```bash
make rust-check
make web-check
make python-check
make config-check
```

The media capability smoke render enters the worker image in Milestone 5, when the typed FFmpeg adapter exists.

## Database changes

Add forward-only SQL files under `db/migrations`:

```text
0002_projects_and_assets.sql
0003_transcript_revisions.sql
0004_renders.sql
```

Keep heavy work outside transactions. A job claim or state transition may be transactional; transcription, FFmpeg execution, and artifact transfer may not.

## Production guardrails

Before deployment outside local development:

- set `ENABLE_DEV_ROUTES=false`;
- reject `AUTH_MODE=disabled` in the production environment;
- replace local development credentials;
- use a persistent artifact store and retention policy;
- configure HTTPS and trusted proxy headers;
- pin container image digests;
- add PostgreSQL backup and restore verification.
