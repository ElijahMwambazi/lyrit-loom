CREATE TABLE jobs (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('system_probe', 'transcribe', 'render')),
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'running', 'cancelling', 'succeeded', 'failed', 'cancelled')
    ),
    phase TEXT NOT NULL,
    progress DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK (progress >= 0 AND progress <= 1),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts > 0),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    result JSONB,
    error JSONB,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner TEXT,
    lease_expires_at TIMESTAMPTZ,
    heartbeat_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ
);

CREATE INDEX jobs_claim_idx
    ON jobs (kind, available_at, created_at)
    WHERE status = 'queued';

CREATE INDEX jobs_expired_lease_idx
    ON jobs (lease_expires_at)
    WHERE status IN ('running', 'cancelling');

CREATE TABLE job_events (
    id BIGSERIAL PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    status TEXT NOT NULL,
    phase TEXT NOT NULL,
    progress DOUBLE PRECISION NOT NULL,
    message TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (job_id, sequence)
);

CREATE INDEX job_events_stream_idx ON job_events (job_id, id);

