CREATE TABLE projects (
    id UUID PRIMARY KEY,
    owner_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 120),
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'ready', 'rendering', 'completed', 'failed')),
    video_width INTEGER NOT NULL DEFAULT 1920 CHECK (video_width BETWEEN 320 AND 3840),
    video_height INTEGER NOT NULL DEFAULT 1080 CHECK (video_height BETWEEN 320 AND 3840),
    video_fps INTEGER NOT NULL DEFAULT 30 CHECK (video_fps IN (24, 25, 30, 50, 60)),
    background_fit TEXT NOT NULL DEFAULT 'cover'
        CHECK (background_fit IN ('cover', 'contain', 'stretch')),
    audio_asset_id UUID,
    background_asset_id UUID,
    active_transcript_revision INTEGER CHECK (active_transcript_revision > 0),
    latest_render_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX projects_owner_updated_idx
    ON projects (owner_id, updated_at DESC, id DESC);

CREATE TABLE assets (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL
        CHECK (kind IN ('audio', 'background', 'normalized_audio', 'subtitle', 'video')),
    storage_key TEXT NOT NULL UNIQUE,
    original_filename TEXT,
    media_type TEXT NOT NULL,
    bytes BIGINT NOT NULL CHECK (bytes >= 0),
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[a-f0-9]{64}$'),
    duration_ms BIGINT CHECK (duration_ms >= 0),
    width INTEGER CHECK (width > 0),
    height INTEGER CHECK (height > 0),
    tool_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX assets_project_kind_idx ON assets (project_id, kind, created_at DESC);
CREATE INDEX assets_sha256_idx ON assets (sha256);

ALTER TABLE projects
    ADD CONSTRAINT projects_audio_asset_fk
        FOREIGN KEY (audio_asset_id) REFERENCES assets(id) ON DELETE SET NULL,
    ADD CONSTRAINT projects_background_asset_fk
        FOREIGN KEY (background_asset_id) REFERENCES assets(id) ON DELETE SET NULL;
