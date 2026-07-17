# Lyrit Loom — Delivery Guide

This guide converts the architecture into implementation order. Each milestone ends in a working product slice and has explicit exit criteria. Build the smallest preset first, then add expressive motion after the media pipeline is trustworthy.

## 1. Development milestones

### Milestone 0 — Foundation and executable contract

**Goal:** one command starts the development system, and the frontend/backend agree on types.

Build:

- Rust workspace with `api`, `worker`, and shared crates;
- React/Vite app and pnpm workspace;
- Python transcriber package with a pinned lockfile and a fake mode;
- PostgreSQL migrations and Docker Compose;
- health/readiness endpoints;
- OpenAPI linting and generated TypeScript client;
- structured logs with request/job correlation IDs;
- CI checks for Rust, TypeScript, Python, OpenAPI, and formatting.

Exit criteria:

- `docker compose up` starts required infrastructure;
- API and worker both report capabilities;
- web app calls `/health/ready` through the generated client;
- CI fails if generated API types drift from `contracts/openapi.yaml`;
- a fake job can be enqueued, claimed, progressed, and completed durably.

Verified on 2026-07-17 with `make check`, `docker compose up --build -d`, the API readiness endpoint, and `make smoke`. The production web proxy delivered the persisted probe event stream through `succeeded`, and the UI contract test confirmed that progress and the terminal worker result are rendered. Milestone 1 should not weaken this foundation path.

### Milestone 1 — Projects and safe media ingestion

**Goal:** create a project and attach one valid audio file and background image.

Project foundation verified on 2026-07-17: project create/list/get/update is durable and owner-scoped, the generated-client React workspace supports create and rename, and the original readiness/probe path remains covered. Media ingestion and activation are the remaining work in this milestone.

Build:

- project create/list/get/update endpoints;
- streaming multipart upload with size limits and SHA-256 calculation;
- local `ArtifactStore` adapter with atomic writes;
- ffprobe-based media inspection;
- project screen with drag/drop upload and progress;
- validation messages for unsupported or corrupt media;
- fixtures for supported and rejected inputs.

Exit criteria:

- large uploads are streamed rather than fully buffered in memory;
- client filenames cannot affect storage paths;
- interrupted uploads never become active assets;
- audio duration and image dimensions are persisted from probe results;
- re-uploading a source asset replaces the active pointer without destroying old bytes inside the transaction boundary.

### Milestone 2 — Whisper transcription

**Goal:** turn active audio into an editable, word-timed transcript revision.

Build:

- FFmpeg normalization to mono 16 kHz PCM WAV for ASR;
- versioned Rust-to-Python JSON contract;
- `faster-whisper` adapter with word timestamps and configurable VAD;
- transcription job handler, progress phases, timeout, cancellation, and error mapping;
- post-processing that converts segments into cues and validates timeline invariants;
- active transcript GET endpoint with ETag;
- transcript review screen with text, confidence hints, and audio playback.

Exit criteria:

- a known fixture produces schema-valid, ordered word timestamps;
- a fake transcriber makes integration tests deterministic and model-free;
- model/runtime metadata is stored with the transcript;
- invalid output from Python fails safely and does not activate a revision;
- CPU mode works for development; GPU configuration changes deployment only, not the contract.

### Milestone 3 — Waveform timeline editor

**Goal:** correct lyrics and timing without losing work or overwriting a newer revision.

Build:

- WaveSurfer waveform and transport controls;
- word/cue selection synchronized with playhead;
- edit text, nudge timing, drag cue bounds, split/merge cue;
- keyboard controls for play/pause, seeking, and timing nudges;
- local dirty state and debounced draft persistence if desired;
- explicit save to immutable revision using `If-Match`;
- conflict UX that preserves local edits on `412`;
- accessible transcript list independent of the visual waveform.

Exit criteria:

- all saved words satisfy timeline invariants;
- the editor remains responsive on a representative full-length song;
- stale saves cannot overwrite a newer revision;
- refresh after save returns the same words/times and a new revision/ETag;
- keyboard-only editing covers the primary review workflow.

### Milestone 4 — Deterministic ASS compiler

**Goal:** compile a transcript revision into a safe, reproducible subtitle artifact.

Build:

- typed ASS document model and serializer;
- style, event, and timestamp formatting;
- strict text escaping and Unicode test cases;
- `clean_karaoke` preset using stable per-word timing;
- cue grouping/layout policy and viewport-safe margins;
- golden-file tests for output scripts;
- debug mapping from transcript word ID to ASS event/line.

Exit criteria:

- identical snapshot input produces byte-identical ASS output;
- braces, backslashes, newlines, emoji, and multilingual text cannot inject override tags;
- rendered word highlighting stays within an agreed tolerance on fixture audio;
- zero/very-short word durations are normalized or rejected consistently;
- the compiler crate has no database, HTTP, or subprocess dependency.

### Milestone 5 — FFmpeg render pipeline

**Goal:** render, cancel, inspect, and download a valid MP4.

Build:

- per-job workspaces with sanitized fixed filenames;
- FFmpeg capability check for libass/subtitles and required encoders;
- background fit modes and fixed-frame-rate canvas generation;
- ASS burn-in, H.264/AAC output, `yuv420p`, and fast-start MP4;
- progress parsing through FFmpeg's machine-readable progress channel;
- cooperative cancellation and child-process termination;
- output ffprobe verification and atomic artifact promotion;
- render manifest and result/download UI.

Exit criteria:

- output duration matches the input audio within an agreed tolerance;
- output decodes, has expected dimensions/fps, H.264 video, AAC audio, and `yuv420p` pixel format;
- cancel stops the child process and no partial output is published;
- a restarted worker recovers or safely retries an expired leased job;
- rendering the same snapshot with the same pinned toolchain produces equivalent output and an identical manifest input section.

### Milestone 6 — Private beta hardening

**Goal:** make failures understandable and operation safe for real projects.

Build:

- authentication/owner scoping, with explicit disabled mode only for local development;
- quotas for input bytes, duration, active jobs, and retained outputs;
- retry policy by error class and dead-job diagnostics;
- metrics for queue latency, phase duration, failures, cancellations, and output size;
- artifact cleanup/retention jobs;
- security review of upload and subprocess boundaries;
- backup/restore test for PostgreSQL and artifact storage;
- browser end-to-end tests for the vertical slice.

Exit criteria:

- one user cannot read another user's project or artifact identifiers;
- logs contain no lyrics, auth tokens, or raw media paths by default;
- every user-facing failure has a stable code and useful recovery instruction;
- restore procedure recovers a project, transcript revision, and final artifact;
- an end-to-end smoke test can create, transcribe, edit, render, and download.

### Milestone 7 — Expressive presets

**Goal:** add kinetic typography without weakening the safe compiler boundary.

Build in this order:

1. `focus_word`: scale/color/opacity emphasis with fixed anchor points;
2. `kinetic_pop`: bounded `\\move`, `\\fad`, and transform behavior;
3. preset preview thumbnails and low-resolution preview renders;
4. versioned preset schema and migration rules.

Exit criteria:

- presets are allow-listed semantic settings, never raw client-provided ASS or filtergraph text;
- each preset has golden ASS and rendered-frame visual regression fixtures;
- older render manifests still identify the exact preset/compiler version used.

## 2. Implementation notes

### 2.1 Rust backend and worker

#### Layering

Keep Axum handlers thin:

```rust
pub async fn start_render(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Path(project_id): Path<ProjectId>,
    IdempotencyKey(key): IdempotencyKey,
    Json(request): Json<StartRenderRequest>,
) -> ApiResult<(StatusCode, Json<RenderAccepted>)> {
    let accepted = state
        .render_service
        .start(user.id, project_id, key, request.into())
        .await?;
    Ok((StatusCode::ACCEPTED, Json(accepted.into())))
}
```

The handler extracts transport data and maps the result. `RenderService` owns authorization checks, idempotency, snapshot validation, and the transaction that creates `render` + `job` rows.

Recommended crate responsibilities:

- `domain`: IDs, entities, state machines, timeline validation, render intent;
- `application`: service methods and ports (`ProjectRepository`, `JobRepository`, `ArtifactStore`, `Transcriber`, `MediaEngine`);
- `persistence`: SQLx implementations and transaction wrapper;
- `api-model`: Serde DTOs matching OpenAPI;
- `api`: route/middleware/composition only;
- `worker`: claim loop and use-case handlers only.

#### Error model

Use an application error enum with stable variants, then one API mapping to `Problem`. Preserve internal error chains in tracing, but send only safe detail and a `diagnostic_id`/`request_id` to the client.

Do not expose:

- SQL/database text;
- absolute server paths;
- FFmpeg command stderr in full;
- Python tracebacks;
- auth/token information.

#### Durable job claiming

A worker loop should:

1. open a short transaction;
2. select an eligible queued job for its supported type using `FOR UPDATE SKIP LOCKED`;
3. update it to `running`, set lease owner/expiry, increment attempt, and append a job event;
4. commit;
5. run work outside the transaction;
6. heartbeat at a bounded interval;
7. complete/fail/requeue in another short transaction.

Use a database clock (`now()`) for leases to avoid host-clock disagreement. Put a hard ceiling on attempts and include randomized backoff. A reaper may requeue expired leases only when the recorded phase is restart-safe; all phases should be idempotent through fixed job workspace keys and atomic final promotion.

#### Idempotency

For each idempotent operation persist:

- owner ID;
- operation name;
- client key;
- hash of the canonical request;
- response status/body or created resource IDs;
- expiration.

The same key + same request returns the original result. The same key + different request returns `409 idempotency_conflict`. Do not treat retries as new renders.

#### Uploads

- apply Tower/Axum body limits before multipart parsing;
- stream chunks to the artifact store while hashing and counting bytes;
- use media-type sniffing and ffprobe facts, not only the browser `Content-Type`;
- allow-list containers/codecs needed by the product;
- reject unexpectedly long audio before transcription;
- store original names only for display and safe `Content-Disposition` generation;
- keep temporary and final object keys under a server-controlled prefix.

#### Configuration

Use one typed config object loaded once. Secrets come from environment/secret management; non-secret defaults may come from a file. Validate on startup:

```text
DATABASE_URL
ARTIFACT_STORE=local|s3
ARTIFACT_ROOT or S3_* settings
FFMPEG_PATH, FFPROBE_PATH
WORKER_QUEUES, WORKER_CONCURRENCY
TRANSCRIBER_COMMAND, TRANSCRIBER_MODEL
MAX_UPLOAD_BYTES, MAX_AUDIO_DURATION_MS
AUTH_MODE=disabled|oidc
```

Keep `AUTH_MODE=disabled` unavailable in production builds or fail startup when paired with a production environment.

### 2.2 React frontend

#### Feature shape

Organize by user capability, not file type:

```text
features/projects      create/list/project shell
features/uploads       source asset cards and progress
features/transcript    waveform, cue list, editor store, save/conflict flow
features/renders       settings, job progress, result/download
```

Within `transcript`, useful components are:

- `WaveformTimeline`: WaveSurfer lifecycle and cue overlays;
- `TransportControls`: play, pause, seek, speed, loop selection;
- `CueList`: virtualized accessible list;
- `CueEditor`: text and start/end editing;
- `WordTimingRow`: fine nudge controls and confidence indication;
- `TranscriptSaveBar`: dirty state, validation, save, conflict recovery.

#### Server state vs editor state

TanStack Query stores canonical resources and job polling. Zustand stores a normalized working copy:

```ts
type EditorState = {
  baseRevision: number;
  baseEtag: string;
  cuesById: Record<string, Cue>;
  cueOrder: string[];
  selectedCueId: string | null;
  selectedWordId: string | null;
  playheadMs: number;
  zoom: number;
  dirty: boolean;
};
```

Do not update React state on every audio frame. Subscribe to WaveSurfer time updates, throttle UI updates, and calculate active word with an indexed/binary-search structure.

#### Waveform editing

Avoid rendering one heavy WaveSurfer region per word for long tracks. Use regions for selected/visible cues and render the complete word list separately. The data model, not the plugin's object graph, remains canonical.

Timing operations should be pure functions with tests:

- `nudgeWord(wordId, deltaMs)`;
- `setCueBounds(cueId, startMs, endMs)`;
- `splitCue(cueId, wordId)`;
- `mergeCue(leftCueId, rightCueId)`;
- `normalizeTimeline(cues, durationMs)`.

Each operation returns validation issues and supports undo/redo through small editor commands or snapshots. Start with a bounded history (for example, the most recent 100 actions).

#### Saving and conflict handling

Save only a validated document. Send `If-Match: <baseEtag>`. On success:

1. replace query cache with returned revision;
2. set the new ETag/revision as base;
3. clear undo history or mark the current point clean;
4. leave playhead/selection intact when IDs still exist.

On `412`, never discard the local working copy. Fetch the new server revision and offer a clear comparison or export of the draft before reload. Multi-user merge can remain deferred, but data loss cannot.

#### Preview

Use a fast CSS/React overlay for approximate typography and active-word emphasis. Label it as preview only in product copy if mismatch is noticeable. For higher fidelity later, enqueue a short, low-resolution preview render around the playhead rather than trying to reproduce every libass behavior in CSS.

#### Job progress

Subscribe to SSE after a `202`. Update the Query cache from events. If the stream errors:

- retain last known progress;
- poll the canonical job URL with backoff;
- reconnect with `Last-Event-ID` when appropriate;
- stop polling/streaming at a terminal state.

Progress is monotonic within an attempt. Show named phases instead of fake precision: Preparing, Transcribing, Building subtitles, Rendering video, Finalizing.

### 2.3 Whisper integration

#### Why a process adapter

Keep Whisper outside the Rust process. Python/CUDA/native model dependencies evolve differently from the API and FFmpeg stack. A versioned file-based JSON contract gives:

- deterministic input/output validation;
- clean cancellation and timeout behavior;
- a fake implementation for tests;
- CPU/GPU deployment flexibility;
- a future path to a remote transcription service without changing the domain.

For the first release, invoke the process directly from the Rust worker. Do not add a network hop until GPU workers need independent deployment.

#### Audio preparation

Use FFmpeg to create an ASR-only intermediate:

- decode the active audio stream;
- mono channel layout;
- 16 kHz sample rate;
- signed 16-bit PCM WAV;
- no loudness normalization by default unless testing proves it improves the selected model;
- preserve the original asset for final rendering.

Probe duration before and after normalization and reject major mismatch. Hash the normalized bytes so retries can reuse a valid cache.

#### Model behavior

`faster-whisper` supports word-level timestamps and VAD. Treat both as model output requiring review, not perfect ground truth. Lyrics are harder than speech because of singing, backing vocals, reverb, stylized pronunciation, and instrumental gaps.

Practical policy:

- let the user specify a language or use auto-detection;
- keep VAD enabled by default but make it a profile setting;
- store confidence/probability as hints, never as acceptance gates alone;
- accept an optional short prompt for artist names or repeated phrases;
- pin the model artifact/revision and runtime dependencies;
- do not silently change the default model for existing deployments;
- surface low-confidence words in the editor;
- keep diarization and source separation out of the MVP.

#### Post-processing

Convert Whisper segments to editor cues with deterministic rules:

1. trim only transport whitespace, preserving punctuation;
2. convert seconds to rounded integer milliseconds once;
3. clamp tiny negative timestamps to zero;
4. ensure `end_ms > start_ms` with a documented minimum word duration;
5. resolve small overlaps using a bounded midpoint rule;
6. group words by source segment, then split cues that exceed configured word count, line length, or duration;
7. generate stable UUIDs for cue/word identities;
8. validate the complete timeline before creating a revision.

Never keep multiple float-to-millisecond conversions across layers; that creates cumulative timing drift.

#### Progress and failure mapping

The Python process writes structured final output. For progress, it may write JSON Lines to a separate progress pipe/file descriptor or emit a restricted prefixed JSON line format that Rust validates. Regular logs stay separate.

Map failures into stable classes:

| Failure                              | Job behavior                                                    |
| ------------------------------------ | --------------------------------------------------------------- |
| Invalid/undecodable normalized audio | terminal                                                        |
| Model missing in production image    | readiness failure; do not claim job                             |
| CUDA out of memory                   | retryable only on a different/lower profile; otherwise terminal |
| Worker/process crash                 | retryable within attempt limit                                  |
| Output schema/timeline invalid       | terminal with diagnostic artifact retained briefly              |
| User cancellation                    | terminate, mark cancelled, clean partial output                 |

### 2.4 ASS generation

#### Compiler design

Build an AST-like writer, not ad hoc string concatenation:

```text
AssDocument
  ScriptInfo
  Styles[]
  Events[]

DialogueEvent
  layer, start, end, style, margins, effect, textParts[]

TextPart
  OverrideTag(allow-listed typed variant)
  EscapedText(user text)
  LineBreak
```

Serialize in one place. This makes it impossible for normal text to become a raw override tag.

#### Timing

ASS karaoke tags commonly use centiseconds while the domain uses milliseconds. Define one rounding policy and test the total:

- convert each word duration using error accumulation so rounding does not make the line drift materially;
- assign any remainder to the final eligible word in the cue;
- clamp animation fades/moves to the event duration;
- make cue lead-in/lead-out explicit rather than hidden constants.

For the first preset, one dialogue event per cue with karaoke timing is easier to keep aligned. For motion-heavy presets, generating per-word events may be appropriate but increases collision and performance complexity.

#### Fonts and layout

- bundle only fonts with suitable licenses;
- refer to allow-listed family names, not uploaded font paths;
- ship the same font files in development, CI render tests, and production workers;
- hash fonts into the render manifest;
- use ASS `PlayResX/PlayResY` matching the target canvas;
- calculate safe margins from resolution/aspect ratio;
- test long words, RTL/complex scripts, emoji fallback, and missing glyph behavior.

### 2.5 FFmpeg pipeline

#### Capability discovery

At startup, execute bounded checks and parse outputs:

- `ffmpeg -version` and `ffprobe -version`;
- filter list contains `subtitles` (requires libass);
- selected H.264 encoder is present;
- selected AAC encoder is present;
- a tiny bundled smoke render succeeds with the configured fonts.

Store the detected version/capabilities. A worker that lacks them must not claim render jobs.

#### Command construction

Use `tokio::process::Command` and pass every argument separately. Never run through a shell. Clients select semantic options which Rust maps to allow-listed arguments.

An illustrative final-render pipeline for a still image is:

```text
ffmpeg -y
  -loop 1 -framerate 30 -i background.png
  -i source-audio
  -vf <typed scale/crop-or-pad>,subtitles=lyrics.ass:fontsdir=fonts
  -c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p
  -c:a aac -b:a 192k
  -shortest -movflags +faststart
  -progress pipe:1 -nostats
  output.partial.mp4
```

This is a design illustration, not a client-configurable command. The adapter constructs the filter graph from typed values and uses fixed job-local filenames to minimize filter-path escaping problems.

Profiles:

| Profile | Suggested intent                                                                                      |
| ------- | ----------------------------------------------------------------------------------------------------- |
| Draft   | reduced resolution, faster preset, higher CRF, same timing/font path                                  |
| Final   | requested resolution, medium/slower preset as capacity allows, quality-tested CRF, AAC bitrate target |

Pin profile names to exact settings in code/config and store the resolved settings in the manifest.

#### Progress

Use `-progress pipe:1 -nostats`, parse `key=value` records, and compute progress from `out_time_ms / expected_duration_ms`. Do not scrape human-readable stderr for progress. Capture a bounded tail of stderr for diagnostics.

Phase allocation can make overall progress monotonic:

```text
validation             0%–5%
normalization/probe    5%–15%
subtitle compilation  15%–20%
encoding              20%–95%
final verification    95%–100%
```

#### Cancellation and isolation

- create a job-specific directory with restrictive permissions;
- materialize inputs under fixed generated names;
- spawn FFmpeg in a controllable process group where supported;
- on cancellation, request graceful termination, wait briefly, then force termination;
- await process exit before workspace cleanup;
- enforce wall-clock timeout, CPU/memory limits at container/orchestrator level, and output-size quota;
- never publish `output.partial.mp4`;
- verify final output with ffprobe, rename/promote, then complete the job.

#### Background fit

Map fit modes deterministically:

- `cover`: scale up preserving aspect ratio, then center crop;
- `contain`: scale down/up preserving aspect ratio, then pad with configured color;
- `stretch`: scale directly; include only because contract exposes it, but do not make it the UI default.

Normalize dimensions to codec-compatible even values before encoding.

## 3. Verification and operations

### 3.1 Test pyramid

| Level               | High-value tests                                                                                          |
| ------------------- | --------------------------------------------------------------------------------------------------------- |
| Domain unit         | timeline invariants, state transitions, retry classification, idempotency request hashing                 |
| ASS unit/golden     | escaping, timing rounding, preset serialization, Unicode, long lines                                      |
| Adapter integration | SQLx repositories against PostgreSQL, local artifact atomicity, fake transcriber contract, ffprobe parser |
| Media integration   | short fixture rendered by real FFmpeg; output probed for streams, duration, dimensions, pixel format      |
| API contract        | request/response examples validated against OpenAPI; generated TypeScript compiles                        |
| Frontend component  | waveform/editor commands, dirty/save/conflict states, job fallback from SSE to polling                    |
| End-to-end          | upload → fake/real short transcription → edit → render → download/play output                             |
| Resilience          | worker killed mid-job, lease expiry, duplicate POST, cancellation race, storage write failure             |

Keep media fixtures short and licensed for repository use. Maintain at least:

- clear spoken/sung English fixture;
- silence and instrumental gap fixture;
- Unicode/multilingual transcript fixture;
- corrupt audio and misleading extension fixture;
- portrait, landscape, tiny, and high-resolution images;
- lyrics containing braces, backslashes, commas, emoji, and line breaks.

### 3.2 CI gates

Run on every change:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
sqlx migration/query verification
pnpm lint && pnpm typecheck && pnpm test
Python lint/type/test for transcriber contract
OpenAPI lint + generated-client drift check
short FFmpeg/libass smoke render in the worker image
```

Run slower visual/media regressions on the main branch or before release. Pin container image digests and lockfiles so CI and production use the same rendering toolchain.

### 3.3 Observability

Every request and job carries IDs through logs/spans:

```text
request_id, owner_id (non-sensitive internal ID), project_id,
job_id, render_id, attempt, phase, worker_id, tool_version
```

Do not record lyric text or original filenames in routine logs.

Metrics:

- HTTP latency/error rate by route and status;
- upload bytes/rejections;
- job queue depth and oldest queued age by type;
- job duration by phase/model/profile;
- success/failure/cancellation/retry counts;
- worker lease expirations;
- render speed ratio (`encoded duration / wall time`);
- artifact bytes and cleanup failures.

Alerts should focus on user impact: no ready worker for a queue, oldest job age, sustained failure rate, database/storage unavailability, and disk pressure.

### 3.4 Security checklist

- authenticate all project/artifact routes in deployed environments;
- scope every repository query by owner, including UUID lookups;
- set upload byte, duration, dimension, job concurrency, and output quotas;
- stream bodies and bound parsers/log capture;
- probe decoded content; never trust extension or MIME alone;
- generate all work/storage paths server-side and prevent symlink traversal;
- invoke subprocesses without a shell and without client-provided raw arguments;
- allow-list fonts, presets, codecs, dimensions, and model profiles;
- run worker containers as non-root with read-only base filesystem and a bounded writable workspace;
- keep model/font/tool images patched and reproducibly pinned;
- redact secrets and lyrics from logs/errors;
- authorize artifact downloads at request time or use short-lived signed URLs later;
- define deletion and retention behavior before storing real user media.

### 3.5 Release definition of done

The first release is complete when a user can:

1. create a private project;
2. upload valid audio and background media with clear progress/errors;
3. start transcription and recover progress after refresh;
4. correct words and timing against a waveform;
5. save without silent revision conflicts;
6. select the `clean_karaoke` preset and start a render;
7. cancel a running job;
8. download and play a verified MP4; and
9. reproduce which inputs, transcript, preset, fonts, and tool versions created that MP4.

Engineering is complete only when the same flow passes through the public contract, durable queue, real ASS compiler, and real FFmpeg adapter—not through route-specific shortcuts.
