use std::str::FromStr;

use async_trait::async_trait;
use lyrit_application::{
    ActivateTranscript, ApplicationError, AssetRepository, JobRepository, NewAsset, NewProject,
    ProjectChanges, ProjectRepository, ReplaceTranscript, StartTranscription, TranscriptRepository,
};
use lyrit_domain::{
    Asset, AssetKind, BackgroundFit, Job, JobError, JobEvent, JobStatus, Project, ProjectStatus,
    TranscriberMetadata, TranscriptCue, TranscriptRevision, VideoSettings,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct ProjectRecord {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    status: String,
    video_width: i32,
    video_height: i32,
    video_fps: i32,
    background_fit: String,
    audio_asset_id: Option<Uuid>,
    background_asset_id: Option<Uuid>,
    active_transcript_revision: Option<i32>,
    latest_render_id: Option<Uuid>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl TryFrom<ProjectRecord> for Project {
    type Error = ApplicationError;

    fn try_from(record: ProjectRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            owner_id: record.owner_id,
            name: record.name,
            status: ProjectStatus::from_str(&record.status)
                .map_err(ApplicationError::InvalidData)?,
            video_settings: VideoSettings {
                width: record.video_width,
                height: record.video_height,
                fps: record.video_fps,
                background_fit: BackgroundFit::from_str(&record.background_fit)
                    .map_err(ApplicationError::InvalidData)?,
            },
            audio_asset_id: record.audio_asset_id,
            background_asset_id: record.background_asset_id,
            active_transcript_revision: record.active_transcript_revision,
            latest_render_id: record.latest_render_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }
}

#[derive(Clone)]
pub struct PgProjectRepository {
    pool: PgPool,
}

impl PgProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProjectRepository for PgProjectRepository {
    async fn create(&self, project: NewProject) -> Result<Project, ApplicationError> {
        sqlx::query_as::<_, ProjectRecord>(
            r#"
            INSERT INTO projects (
                id, owner_id, name, video_width, video_height, video_fps, background_fit
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, owner_id, name, status, video_width, video_height, video_fps,
                      background_fit, audio_asset_id, background_asset_id,
                      active_transcript_revision, latest_render_id, created_at, updated_at
            "#,
        )
        .bind(project.id)
        .bind(project.owner_id)
        .bind(project.name)
        .bind(project.video_settings.width)
        .bind(project.video_settings.height)
        .bind(project.video_settings.fps)
        .bind(project.video_settings.background_fit.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(repository_error)
        .and_then(Project::try_from)
    }

    async fn get(&self, owner_id: Uuid, id: Uuid) -> Result<Option<Project>, ApplicationError> {
        sqlx::query_as::<_, ProjectRecord>(
            r#"
            SELECT id, owner_id, name, status, video_width, video_height, video_fps,
                   background_fit, audio_asset_id, background_asset_id,
                   active_transcript_revision, latest_render_id, created_at, updated_at
            FROM projects
            WHERE owner_id = $1 AND id = $2
            "#,
        )
        .bind(owner_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_error)?
        .map(Project::try_from)
        .transpose()
    }

    async fn list(
        &self,
        owner_id: Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Project>, ApplicationError> {
        sqlx::query_as::<_, ProjectRecord>(
            r#"
            SELECT id, owner_id, name, status, video_width, video_height, video_fps,
                   background_fit, audio_asset_id, background_asset_id,
                   active_transcript_revision, latest_render_id, created_at, updated_at
            FROM projects
            WHERE owner_id = $1
            ORDER BY updated_at DESC, id DESC
            OFFSET $2
            LIMIT $3
            "#,
        )
        .bind(owner_id)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(repository_error)?
        .into_iter()
        .map(Project::try_from)
        .collect()
    }

    async fn update(
        &self,
        owner_id: Uuid,
        id: Uuid,
        changes: ProjectChanges,
    ) -> Result<Option<Project>, ApplicationError> {
        let (width, height, fps, background_fit) = changes
            .video_settings
            .map(|settings| {
                (
                    Some(settings.width),
                    Some(settings.height),
                    Some(settings.fps),
                    Some(settings.background_fit.as_str()),
                )
            })
            .unwrap_or((None, None, None, None));
        sqlx::query_as::<_, ProjectRecord>(
            r#"
            UPDATE projects
            SET name = COALESCE($3, name),
                video_width = COALESCE($4, video_width),
                video_height = COALESCE($5, video_height),
                video_fps = COALESCE($6, video_fps),
                background_fit = COALESCE($7, background_fit),
                updated_at = now()
            WHERE owner_id = $1 AND id = $2
            RETURNING id, owner_id, name, status, video_width, video_height, video_fps,
                      background_fit, audio_asset_id, background_asset_id,
                      active_transcript_revision, latest_render_id, created_at, updated_at
            "#,
        )
        .bind(owner_id)
        .bind(id)
        .bind(changes.name)
        .bind(width)
        .bind(height)
        .bind(fps)
        .bind(background_fit)
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_error)?
        .map(Project::try_from)
        .transpose()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AssetRecord {
    id: Uuid,
    project_id: Uuid,
    kind: String,
    storage_key: String,
    original_filename: Option<String>,
    media_type: String,
    bytes: i64,
    sha256: String,
    duration_ms: Option<i64>,
    width: Option<i32>,
    height: Option<i32>,
    tool_metadata: Value,
    created_at: OffsetDateTime,
}

impl TryFrom<AssetRecord> for Asset {
    type Error = ApplicationError;

    fn try_from(record: AssetRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            project_id: record.project_id,
            kind: AssetKind::from_str(&record.kind).map_err(ApplicationError::InvalidData)?,
            storage_key: record.storage_key,
            original_filename: record.original_filename,
            media_type: record.media_type,
            bytes: record.bytes,
            sha256: record.sha256,
            duration_ms: record.duration_ms,
            width: record.width,
            height: record.height,
            tool_metadata: record.tool_metadata,
            created_at: record.created_at,
        })
    }
}

#[derive(Clone)]
pub struct PgAssetRepository {
    pool: PgPool,
}

impl PgAssetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const ASSET_COLUMNS: &str = r#"
    id, project_id, kind, storage_key, original_filename, media_type, bytes, sha256,
    duration_ms, width, height, tool_metadata, created_at
"#;

#[async_trait]
impl AssetRepository for PgAssetRepository {
    async fn get(&self, id: Uuid) -> Result<Option<Asset>, ApplicationError> {
        let query = format!("SELECT {ASSET_COLUMNS} FROM assets WHERE id = $1");
        sqlx::query_as::<_, AssetRecord>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(repository_error)?
            .map(Asset::try_from)
            .transpose()
    }

    async fn get_for_owner(
        &self,
        owner_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Asset>, ApplicationError> {
        let query = format!(
            r#"
            SELECT {ASSET_COLUMNS}
            FROM assets
            WHERE id = $2
              AND project_id IN (SELECT id FROM projects WHERE owner_id = $1)
            "#
        );
        sqlx::query_as::<_, AssetRecord>(&query)
            .bind(owner_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(repository_error)?
            .map(Asset::try_from)
            .transpose()
    }

    async fn activate(
        &self,
        owner_id: Uuid,
        asset: NewAsset,
    ) -> Result<Option<Asset>, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let project = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM projects WHERE id = $1 AND owner_id = $2 FOR UPDATE",
        )
        .bind(asset.project_id)
        .bind(owner_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?;
        if project.is_none() {
            transaction.rollback().await.map_err(repository_error)?;
            return Ok(None);
        }

        let query = format!(
            r#"
            INSERT INTO assets (
                id, project_id, kind, storage_key, original_filename, media_type, bytes, sha256,
                duration_ms, width, height, tool_metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING {ASSET_COLUMNS}
            "#
        );
        let record = sqlx::query_as::<_, AssetRecord>(&query)
            .bind(asset.id)
            .bind(asset.project_id)
            .bind(asset.kind.as_str())
            .bind(asset.storage_key)
            .bind(asset.original_filename)
            .bind(asset.media_type)
            .bind(asset.bytes)
            .bind(asset.sha256)
            .bind(asset.duration_ms)
            .bind(asset.width)
            .bind(asset.height)
            .bind(asset.tool_metadata)
            .fetch_one(&mut *transaction)
            .await
            .map_err(repository_error)?;

        let pointer = match asset.kind {
            AssetKind::Audio => "audio_asset_id",
            AssetKind::Background => "background_asset_id",
            _ => {
                transaction.rollback().await.map_err(repository_error)?;
                return Err(ApplicationError::Validation(
                    "only source assets can be activated on a project".to_owned(),
                ));
            }
        };
        let update = format!(
            r#"
            UPDATE projects
            SET {pointer} = $2,
                status = CASE
                    WHEN {other_pointer} IS NOT NULL THEN 'ready'
                    ELSE 'draft'
                END,
                active_transcript_revision = CASE
                    WHEN $3 = 'audio' THEN NULL
                    ELSE active_transcript_revision
                END,
                latest_render_id = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
            other_pointer = if asset.kind == AssetKind::Audio {
                "background_asset_id"
            } else {
                "audio_asset_id"
            }
        );
        sqlx::query(&update)
            .bind(asset.project_id)
            .bind(asset.id)
            .bind(asset.kind.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(repository_error)?;
        transaction.commit().await.map_err(repository_error)?;
        Asset::try_from(record).map(Some)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TranscriptRecord {
    id: Uuid,
    project_id: Uuid,
    audio_asset_id: Uuid,
    job_id: Option<Uuid>,
    revision: i32,
    source: String,
    language: String,
    duration_ms: i64,
    cues: Value,
    transcriber: Option<Value>,
    created_at: OffsetDateTime,
}

impl TryFrom<TranscriptRecord> for TranscriptRevision {
    type Error = ApplicationError;

    fn try_from(record: TranscriptRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            project_id: record.project_id,
            audio_asset_id: record.audio_asset_id,
            job_id: record.job_id,
            revision: record.revision,
            source: record.source,
            language: record.language,
            duration_ms: record.duration_ms,
            cues: serde_json::from_value::<Vec<TranscriptCue>>(record.cues)
                .map_err(|error| ApplicationError::InvalidData(error.to_string()))?,
            transcriber: record
                .transcriber
                .map(serde_json::from_value::<TranscriberMetadata>)
                .transpose()
                .map_err(|error| ApplicationError::InvalidData(error.to_string()))?,
            created_at: record.created_at,
        })
    }
}

#[derive(Clone)]
pub struct PgTranscriptRepository {
    pool: PgPool,
}

impl PgTranscriptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const TRANSCRIPT_COLUMNS: &str = r#"
    id, project_id, audio_asset_id, job_id, revision, source, language,
    duration_ms, cues, transcriber, created_at
"#;

#[async_trait]
impl TranscriptRepository for PgTranscriptRepository {
    async fn enqueue(&self, request: StartTranscription) -> Result<Option<Job>, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let source = sqlx::query_as::<_, (Uuid, String, i64)>(
            r#"
            SELECT asset.id, asset.storage_key, asset.duration_ms
            FROM projects AS project
            JOIN assets AS asset ON asset.id = project.audio_asset_id
            WHERE project.id = $1 AND project.owner_id = $2
            FOR UPDATE OF project
            "#,
        )
        .bind(request.project_id)
        .bind(request.owner_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?;
        let Some((audio_asset_id, storage_key, duration_ms)) = source else {
            transaction.rollback().await.map_err(repository_error)?;
            return Ok(None);
        };

        if let Some(record) = sqlx::query_as::<_, JobRecord>(
            r#"
            SELECT id, kind, status, phase, progress, attempt, max_attempts,
                   payload, result, error, lease_owner, created_at, started_at, finished_at
            FROM jobs
            WHERE owner_id = $1 AND project_id = $2 AND kind = 'transcribe'
              AND idempotency_key = $3
            "#,
        )
        .bind(request.owner_id)
        .bind(request.project_id)
        .bind(&request.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?
        {
            transaction.commit().await.map_err(repository_error)?;
            return Job::try_from(record).map(Some);
        }

        let payload = serde_json::json!({
            "project_id": request.project_id,
            "audio_asset_id": audio_asset_id,
            "source_storage_key": storage_key,
            "source_duration_ms": duration_ms,
            "language": request.language,
            "model": request.model,
            "initial_prompt": request.initial_prompt,
            "vad_enabled": request.vad_enabled
        });
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            INSERT INTO jobs (
                id, kind, status, phase, progress, payload, max_attempts,
                owner_id, project_id, idempotency_key
            )
            VALUES ($1, 'transcribe', 'queued', 'queued', 0, $2, 3, $3, $4, $5)
            RETURNING id, kind, status, phase, progress, attempt, max_attempts,
                      payload, result, error, lease_owner, created_at, started_at, finished_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(payload)
        .bind(request.owner_id)
        .bind(request.project_id)
        .bind(request.idempotency_key)
        .fetch_one(&mut *transaction)
        .await
        .map_err(repository_error)?;
        let job = Job::try_from(record)?;
        append_event(&mut transaction, &job, Some("Transcription queued")).await?;
        transaction.commit().await.map_err(repository_error)?;
        Ok(Some(job))
    }

    async fn get_active(
        &self,
        owner_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<TranscriptRevision>, ApplicationError> {
        let query = format!(
            r#"
            SELECT {TRANSCRIPT_COLUMNS}
            FROM transcript_revisions
            WHERE project_id = $2
              AND revision = (
                  SELECT active_transcript_revision
                  FROM projects
                  WHERE owner_id = $1 AND id = $2
              )
            "#
        );
        sqlx::query_as::<_, TranscriptRecord>(&query)
            .bind(owner_id)
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(repository_error)?
            .map(TranscriptRevision::try_from)
            .transpose()
    }

    async fn activate(
        &self,
        transcript: ActivateTranscript,
    ) -> Result<TranscriptRevision, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let existing_query =
            format!("SELECT {TRANSCRIPT_COLUMNS} FROM transcript_revisions WHERE job_id = $1");
        if let Some(existing) = sqlx::query_as::<_, TranscriptRecord>(&existing_query)
            .bind(transcript.job_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(repository_error)?
        {
            transaction.commit().await.map_err(repository_error)?;
            return TranscriptRevision::try_from(existing);
        }

        let active_audio = sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT audio_asset_id FROM projects WHERE id = $1 FOR UPDATE",
        )
        .bind(transcript.project_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?
        .flatten();
        if active_audio != Some(transcript.audio_asset_id) {
            transaction.rollback().await.map_err(repository_error)?;
            return Err(ApplicationError::Conflict(
                "active audio changed while transcription was running".to_owned(),
            ));
        }
        let revision = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM transcript_revisions WHERE project_id = $1",
        )
        .bind(transcript.project_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(repository_error)?;
        let query = format!(
            r#"
            INSERT INTO transcript_revisions (
                id, project_id, audio_asset_id, job_id, revision, source, language,
                duration_ms, cues, transcriber
            )
            VALUES ($1, $2, $3, $4, $5, 'whisper', $6, $7, $8, $9)
            RETURNING {TRANSCRIPT_COLUMNS}
            "#
        );
        let record = sqlx::query_as::<_, TranscriptRecord>(&query)
            .bind(Uuid::new_v4())
            .bind(transcript.project_id)
            .bind(transcript.audio_asset_id)
            .bind(transcript.job_id)
            .bind(revision)
            .bind(transcript.language)
            .bind(transcript.duration_ms)
            .bind(serde_json::to_value(transcript.cues).map_err(invalid_data_error)?)
            .bind(serde_json::to_value(transcript.transcriber).map_err(invalid_data_error)?)
            .fetch_one(&mut *transaction)
            .await
            .map_err(repository_error)?;
        sqlx::query(
            "UPDATE projects SET active_transcript_revision = $2, updated_at = now() WHERE id = $1",
        )
        .bind(transcript.project_id)
        .bind(revision)
        .execute(&mut *transaction)
        .await
        .map_err(repository_error)?;
        transaction.commit().await.map_err(repository_error)?;
        TranscriptRevision::try_from(record)
    }

    async fn replace(
        &self,
        transcript: ReplaceTranscript,
    ) -> Result<Option<TranscriptRevision>, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let project_state = sqlx::query_as::<_, (Option<Uuid>, Option<i32>)>(
            r#"
            SELECT audio_asset_id, active_transcript_revision
            FROM projects
            WHERE id = $1 AND owner_id = $2
            FOR UPDATE
            "#,
        )
        .bind(transcript.project_id)
        .bind(transcript.owner_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?;
        let Some((audio_asset_id, active_revision)) = project_state else {
            transaction.rollback().await.map_err(repository_error)?;
            return Ok(None);
        };
        if active_revision != Some(transcript.expected_revision) {
            transaction.rollback().await.map_err(repository_error)?;
            return Err(ApplicationError::RevisionConflict);
        }
        let audio_asset_id = audio_asset_id.ok_or_else(|| {
            ApplicationError::InvalidData("active transcript project has no audio".to_owned())
        })?;
        let base_query = format!(
            "SELECT {TRANSCRIPT_COLUMNS} FROM transcript_revisions WHERE project_id = $1 AND revision = $2"
        );
        let base = sqlx::query_as::<_, TranscriptRecord>(&base_query)
            .bind(transcript.project_id)
            .bind(transcript.expected_revision)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(repository_error)?
            .ok_or_else(|| {
                ApplicationError::InvalidData(
                    "active transcript revision does not exist".to_owned(),
                )
            })?;
        let next_revision = transcript.expected_revision.checked_add(1).ok_or_else(|| {
            ApplicationError::InvalidData("transcript revision overflow".to_owned())
        })?;
        let query = format!(
            r#"
            INSERT INTO transcript_revisions (
                id, project_id, audio_asset_id, job_id, revision, source, language,
                duration_ms, cues, transcriber
            )
            VALUES ($1, $2, $3, NULL, $4, 'edited', $5, $6, $7, $8)
            RETURNING {TRANSCRIPT_COLUMNS}
            "#
        );
        let record = sqlx::query_as::<_, TranscriptRecord>(&query)
            .bind(Uuid::new_v4())
            .bind(transcript.project_id)
            .bind(audio_asset_id)
            .bind(next_revision)
            .bind(transcript.language)
            .bind(transcript.duration_ms)
            .bind(serde_json::to_value(transcript.cues).map_err(invalid_data_error)?)
            .bind(base.transcriber)
            .fetch_one(&mut *transaction)
            .await
            .map_err(repository_error)?;
        let updated = sqlx::query(
            r#"
            UPDATE projects
            SET active_transcript_revision = $3, updated_at = now()
            WHERE id = $1 AND active_transcript_revision = $2
            "#,
        )
        .bind(transcript.project_id)
        .bind(transcript.expected_revision)
        .bind(next_revision)
        .execute(&mut *transaction)
        .await
        .map_err(repository_error)?;
        if updated.rows_affected() != 1 {
            transaction.rollback().await.map_err(repository_error)?;
            return Err(ApplicationError::RevisionConflict);
        }
        transaction.commit().await.map_err(repository_error)?;
        TranscriptRevision::try_from(record).map(Some)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct JobRecord {
    id: Uuid,
    kind: String,
    status: String,
    phase: String,
    progress: f64,
    attempt: i32,
    max_attempts: i32,
    payload: Value,
    result: Option<Value>,
    error: Option<Value>,
    lease_owner: Option<String>,
    created_at: OffsetDateTime,
    started_at: Option<OffsetDateTime>,
    finished_at: Option<OffsetDateTime>,
}

impl TryFrom<JobRecord> for Job {
    type Error = ApplicationError;

    fn try_from(record: JobRecord) -> Result<Self, Self::Error> {
        let status = JobStatus::from_str(&record.status).map_err(ApplicationError::InvalidData)?;
        let error = record
            .error
            .map(serde_json::from_value::<JobError>)
            .transpose()
            .map_err(|error| ApplicationError::InvalidData(error.to_string()))?;

        Ok(Self {
            id: record.id,
            kind: record.kind,
            status,
            phase: record.phase,
            progress: record.progress,
            attempt: record.attempt,
            max_attempts: record.max_attempts,
            payload: record.payload,
            result: record.result,
            error,
            lease_owner: record.lease_owner,
            created_at: record.created_at,
            started_at: record.started_at,
            finished_at: record.finished_at,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct JobEventRecord {
    id: i64,
    job_id: Uuid,
    sequence: i32,
    status: String,
    phase: String,
    progress: f64,
    message: Option<String>,
    occurred_at: OffsetDateTime,
}

impl TryFrom<JobEventRecord> for JobEvent {
    type Error = ApplicationError;

    fn try_from(record: JobEventRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            job_id: record.job_id,
            sequence: record.sequence,
            status: JobStatus::from_str(&record.status).map_err(ApplicationError::InvalidData)?,
            phase: record.phase,
            progress: record.progress,
            message: record.message,
            occurred_at: record.occurred_at,
        })
    }
}

#[derive(Clone)]
pub struct PgJobRepository {
    pool: PgPool,
}

impl PgJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobRepository for PgJobRepository {
    async fn ping(&self) -> Result<(), ApplicationError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| ())
            .map_err(repository_error)
    }

    async fn enqueue_probe(&self, payload: Value) -> Result<Job, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            INSERT INTO jobs (id, kind, status, phase, progress, payload, max_attempts)
            VALUES ($1, 'system_probe', 'queued', 'queued', 0, $2, 1)
            RETURNING id, kind, status, phase, progress, attempt, max_attempts,
                      payload, result, error, lease_owner, created_at, started_at, finished_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(payload)
        .fetch_one(&mut *transaction)
        .await
        .map_err(repository_error)?;
        let job = Job::try_from(record)?;
        append_event(&mut transaction, &job, Some("Job queued")).await?;
        transaction.commit().await.map_err(repository_error)?;
        Ok(job)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Job>, ApplicationError> {
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            SELECT id, kind, status, phase, progress, attempt, max_attempts,
                   payload, result, error, lease_owner, created_at, started_at, finished_at
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_error)?;

        record.map(Job::try_from).transpose()
    }

    async fn claim_next(
        &self,
        worker_id: &str,
        supported_kinds: &[String],
        lease_seconds: i32,
    ) -> Result<Option<Job>, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            WITH candidate AS (
                SELECT id
                FROM jobs
                WHERE status = 'queued'
                  AND available_at <= now()
                  AND kind = ANY($2::text[])
                ORDER BY created_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE jobs AS job
            SET status = 'running',
                phase = 'validating',
                attempt = attempt + 1,
                lease_owner = $1,
                lease_expires_at = now() + ($3::int * INTERVAL '1 second'),
                heartbeat_at = now(),
                started_at = COALESCE(started_at, now())
            FROM candidate
            WHERE job.id = candidate.id
            RETURNING job.id, job.kind, job.status, job.phase, job.progress,
                      job.attempt, job.max_attempts, job.payload, job.result, job.error,
                      job.lease_owner, job.created_at, job.started_at, job.finished_at
            "#,
        )
        .bind(worker_id)
        .bind(supported_kinds.to_vec())
        .bind(lease_seconds)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?;

        let Some(record) = record else {
            transaction.commit().await.map_err(repository_error)?;
            return Ok(None);
        };

        let job = Job::try_from(record)?;
        append_event(&mut transaction, &job, Some("Worker claimed job")).await?;
        transaction.commit().await.map_err(repository_error)?;
        Ok(Some(job))
    }

    async fn update_progress(
        &self,
        id: Uuid,
        worker_id: &str,
        phase: &str,
        progress: f64,
        message: Option<&str>,
        lease_seconds: i32,
    ) -> Result<Job, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            UPDATE jobs
            SET phase = $3,
                progress = GREATEST(progress, $4),
                heartbeat_at = now(),
                lease_expires_at = now() + ($5::int * INTERVAL '1 second')
            WHERE id = $1 AND lease_owner = $2 AND status = 'running'
            RETURNING id, kind, status, phase, progress, attempt, max_attempts,
                      payload, result, error, lease_owner, created_at, started_at, finished_at
            "#,
        )
        .bind(id)
        .bind(worker_id)
        .bind(phase)
        .bind(progress)
        .bind(lease_seconds)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?
        .ok_or(ApplicationError::NotFound)?;

        let job = Job::try_from(record)?;
        append_event(&mut transaction, &job, message).await?;
        transaction.commit().await.map_err(repository_error)?;
        Ok(job)
    }

    async fn complete(
        &self,
        id: Uuid,
        worker_id: &str,
        result: Value,
    ) -> Result<Job, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            UPDATE jobs
            SET status = 'succeeded', phase = 'complete', progress = 1,
                result = $3, error = NULL, finished_at = now(),
                lease_owner = NULL, lease_expires_at = NULL, heartbeat_at = now()
            WHERE id = $1 AND lease_owner = $2 AND status = 'running'
            RETURNING id, kind, status, phase, progress, attempt, max_attempts,
                      payload, result, error, lease_owner, created_at, started_at, finished_at
            "#,
        )
        .bind(id)
        .bind(worker_id)
        .bind(result)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?
        .ok_or(ApplicationError::NotFound)?;

        let job = Job::try_from(record)?;
        append_event(&mut transaction, &job, Some("Job completed")).await?;
        transaction.commit().await.map_err(repository_error)?;
        Ok(job)
    }

    async fn fail(
        &self,
        id: Uuid,
        worker_id: &str,
        error: JobError,
    ) -> Result<Job, ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(repository_error)?;
        let error_json = serde_json::to_value(error)
            .map_err(|error| ApplicationError::InvalidData(error.to_string()))?;
        let record = sqlx::query_as::<_, JobRecord>(
            r#"
            UPDATE jobs
            SET status = 'failed', phase = 'failed', error = $3,
                finished_at = now(), lease_owner = NULL,
                lease_expires_at = NULL, heartbeat_at = now()
            WHERE id = $1 AND lease_owner = $2 AND status = 'running'
            RETURNING id, kind, status, phase, progress, attempt, max_attempts,
                      payload, result, error, lease_owner, created_at, started_at, finished_at
            "#,
        )
        .bind(id)
        .bind(worker_id)
        .bind(error_json)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_error)?
        .ok_or(ApplicationError::NotFound)?;

        let job = Job::try_from(record)?;
        append_event(&mut transaction, &job, Some("Job failed")).await?;
        transaction.commit().await.map_err(repository_error)?;
        Ok(job)
    }

    async fn events_after(
        &self,
        id: Uuid,
        after_event_id: i64,
        limit: i64,
    ) -> Result<Vec<JobEvent>, ApplicationError> {
        let records = sqlx::query_as::<_, JobEventRecord>(
            r#"
            SELECT id, job_id, sequence, status, phase, progress, message, occurred_at
            FROM job_events
            WHERE job_id = $1 AND id > $2
            ORDER BY id
            LIMIT $3
            "#,
        )
        .bind(id)
        .bind(after_event_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(repository_error)?;

        records.into_iter().map(JobEvent::try_from).collect()
    }
}

async fn append_event(
    transaction: &mut Transaction<'_, Postgres>,
    job: &Job,
    message: Option<&str>,
) -> Result<(), ApplicationError> {
    sqlx::query(
        r#"
        INSERT INTO job_events (job_id, sequence, status, phase, progress, message)
        SELECT $1, COALESCE(MAX(sequence), 0) + 1, $2, $3, $4, $5
        FROM job_events
        WHERE job_id = $1
        "#,
    )
    .bind(job.id)
    .bind(job.status.as_str())
    .bind(&job.phase)
    .bind(job.progress)
    .bind(message)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(repository_error)
}

fn repository_error(error: sqlx::Error) -> ApplicationError {
    ApplicationError::Repository(error.to_string())
}

fn invalid_data_error(error: serde_json::Error) -> ApplicationError {
    ApplicationError::InvalidData(error.to_string())
}
