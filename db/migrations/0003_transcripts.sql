ALTER TABLE jobs
    ADD COLUMN owner_id UUID,
    ADD COLUMN project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    ADD COLUMN idempotency_key TEXT;

CREATE UNIQUE INDEX jobs_transcription_idempotency_idx
    ON jobs (owner_id, project_id, kind, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE transcript_revisions (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    audio_asset_id UUID NOT NULL REFERENCES assets(id),
    job_id UUID REFERENCES jobs(id),
    revision INTEGER NOT NULL CHECK (revision > 0),
    source TEXT NOT NULL CHECK (source IN ('whisper', 'edited', 'imported')),
    language TEXT NOT NULL CHECK (char_length(language) BETWEEN 1 AND 64),
    duration_ms BIGINT NOT NULL CHECK (duration_ms > 0),
    cues JSONB NOT NULL,
    transcriber JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, revision),
    UNIQUE (job_id)
);

CREATE INDEX transcript_revisions_project_idx
    ON transcript_revisions (project_id, revision DESC);
