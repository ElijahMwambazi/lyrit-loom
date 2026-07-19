# Lyrit Loom — Product Roadmap

This document is the project status dashboard. It records what is verified, what is currently in focus, and what remains. See [`DELIVERY_GUIDE.md`](DELIVERY_GUIDE.md) for detailed implementation notes and exit criteria.

Last updated: 2026-07-19

## Status at a glance

| Milestone | Status | Product outcome |
| --- | --- | --- |
| 0 — Foundation | Complete | The full local stack starts, reports readiness, and completes durable jobs. |
| 1 — Projects and media ingestion | Complete | Projects accept validated audio and background source media. |
| 2 — Transcription | Complete | Active audio becomes a reviewable word-timed transcript revision. |
| 3 — Timeline editor | Current focus | Lyrics and word timing can be corrected safely. |
| 4 — ASS compiler | Upcoming | Transcript snapshots compile into deterministic subtitles. |
| 5 — Render pipeline | Upcoming | Projects render into verified downloadable MP4 files. |
| 6 — Private beta hardening | Upcoming | Real users and projects are isolated, observable, and recoverable. |
| 7 — Expressive presets | Upcoming | Safe kinetic typography presets expand the visual language. |

## Completed

### Milestone 0 — Foundation

- [x] Create the Rust, React, Python, and pnpm monorepo foundation.
- [x] Establish the modular monolith and separate durable worker.
- [x] Use PostgreSQL as the durable queue with leases, progress, and terminal events.
- [x] Provide liveness and database-backed readiness endpoints.
- [x] Keep the deterministic fake transcriber as the default adapter.
- [x] Generate the TypeScript API client from OpenAPI.
- [x] Validate Rust, TypeScript, Python, OpenAPI, JSON Schema, configuration, and shell scripts with `make check`.
- [x] Build and start the full stack with `docker compose up --build -d`.
- [x] Complete the durable queue probe with `make smoke`.
- [x] Establish Lyrit Loom branding and the tagline “Weave words into motion.”

Verified: 2026-07-19.

### Milestone 1 — Projects and safe media ingestion

- [x] Create, list, read, and rename durable projects.
- [x] Stream multipart uploads without buffering entire media files in HTTP handlers.
- [x] Store source artifacts atomically under generated, path-safe keys.
- [x] Calculate and persist SHA-256 checksums and byte counts.
- [x] Inspect audio and images with bounded ffprobe processes.
- [x] Reject unsupported, corrupt, oversized, or disguised media safely.
- [x] Persist audio duration and background dimensions from probe results.
- [x] Activate replacement assets transactionally while preserving older objects.
- [x] Share the artifact volume between the API and durable worker.
- [x] Show drag-and-drop uploads, progress, metadata, and validation feedback in the web workspace.
- [x] Apply the dark creative-workstation theme and temporary `LL` brand placeholder.
- [x] Verify real MP3 and PNG uploads through the production web proxy.

Verified: 2026-07-19.

### Milestone 2 — Audio normalization and transcription

This milestone established a deterministic, model-free path through the existing worker and fake transcriber before optional Whisper inference is introduced.

- [x] Define the versioned Rust-to-Python transcriber contract and fake adapter.
- [x] Provide durable job claiming, progress events, leases, and worker recovery primitives.
- [x] Persist the active source-audio pointer and probed duration needed by the job.
- [x] Add a transcription job type and API command that enqueues work without running it in the HTTP request.
- [x] Snapshot the active audio asset when the transcription job is created.
- [x] Normalize source audio to mono 16 kHz PCM WAV with FFmpeg inside the worker workspace.
- [x] Report normalization and transcription phases through durable job events.
- [x] Invoke the fake transcriber through the versioned JSON contract.
- [x] Validate transcriber output and reject invalid or unordered word timestamps.
- [x] Persist immutable transcript revisions and their runtime metadata.
- [x] Add the active transcript endpoint with ETag support.
- [x] Add transcript review, confidence hints, and audio playback to the web workspace.
- [x] Cover queue-to-transcript completion with deterministic integration and UI tests.

Verified through the production web proxy and Compose worker on 2026-07-19. Repeated enqueueing returned the original job; the active revision returned a revision ETag and six ordered fake words. Authorized audio returned checksum-identical full content, exact partial ranges for seeking, and `416` for invalid ranges; the web review renders timed words, confidence hints, and synchronized source playback.

## Current focus

### Milestone 3 — Waveform timeline editor

- [ ] Add waveform, transport controls, and synchronized word selection.
- [x] Add accessible word text editing and direct/±50 ms timing controls.
- [ ] Add cue-bound editing, split, and merge operations.
- [x] Save immutable revisions with optimistic concurrency and conflict recovery.
- [ ] Provide a keyboard-accessible transcript editing workflow.

First editor slice verified through the production web proxy on 2026-07-19. Revision 1 remained unchanged after revision 2 became active; stale, missing-precondition, and invalid-timeline saves returned `412`, `428`, and `422` respectively.

## Upcoming

### Milestone 4 — Deterministic ASS compiler

- [ ] Build a typed ASS model, serializer, and strict text escaping.
- [ ] Implement the first stable `clean_karaoke` preset.
- [ ] Add deterministic golden files and Unicode/injection fixtures.
- [ ] Preserve traceability from transcript words to generated ASS events.

### Milestone 5 — FFmpeg render pipeline

- [ ] Create isolated per-job render workspaces and capability checks.
- [ ] Compose background media, ASS subtitles, and audio into H.264/AAC MP4.
- [ ] Parse machine-readable progress and support cooperative cancellation.
- [ ] Verify output with ffprobe before atomic publication.
- [ ] Add render status, manifest, and download UI.

### Milestone 6 — Private beta hardening

- [ ] Add authentication and enforce owner isolation.
- [ ] Add quotas, retention, cleanup jobs, and classified retry policies.
- [ ] Add operational metrics, safe diagnostics, and backup/restore verification.
- [ ] Add browser-level tests for the complete product workflow.

### Milestone 7 — Expressive presets

- [ ] Add the `focus_word` preset with bounded emphasis behavior.
- [ ] Add the `kinetic_pop` preset with safe motion primitives.
- [ ] Add low-resolution previews and preset thumbnails.
- [ ] Version the preset schema and add visual regression fixtures.

## Roadmap maintenance

- Update this file in the same change that starts or completes roadmap work.
- Mark an item complete only after its relevant automated and end-to-end checks pass.
- Keep only one milestone as the current focus unless work is explicitly split.
- Record verification dates for completed milestones.
- Put detailed design decisions in an ADR and implementation guidance in the delivery guide instead of expanding this dashboard indefinitely.
