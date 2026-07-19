use lyrit_domain::{
    Asset, BackgroundFit, Job, JobEvent, Project, TranscriberMetadata, TranscriptCue,
    TranscriptRevision, VideoSettings,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub checks: Vec<ReadinessCheck>,
}

#[derive(Debug, Serialize)]
pub struct ReadinessCheck {
    pub name: &'static str,
    pub ready: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Problem {
    pub r#type: String,
    pub title: String,
    pub status: u16,
    pub code: String,
    pub detail: String,
    pub request_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectRequest {
    pub name: String,
    pub video_settings: Option<VideoSettingsRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub video_settings: Option<VideoSettingsRequest>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VideoSettingsRequest {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub background_fit: BackgroundFit,
}

impl From<VideoSettingsRequest> for VideoSettings {
    fn from(settings: VideoSettingsRequest) -> Self {
        Self {
            width: settings.width,
            height: settings.height,
            fps: settings.fps,
            background_fit: settings.background_fit,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub video_settings: VideoSettingsResponse,
    pub audio_asset: Option<AssetResponse>,
    pub background_asset: Option<AssetResponse>,
    pub active_transcript_revision: Option<i32>,
    pub latest_render_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Project> for ProjectResponse {
    fn from(project: Project) -> Self {
        Self::with_assets(project, None, None)
    }
}

impl ProjectResponse {
    pub fn with_assets(
        project: Project,
        audio_asset: Option<Asset>,
        background_asset: Option<Asset>,
    ) -> Self {
        Self {
            id: project.id,
            name: project.name,
            status: project.status.to_string(),
            video_settings: project.video_settings.into(),
            audio_asset: audio_asset.map(Into::into),
            background_asset: background_asset.map(Into::into),
            active_transcript_revision: project.active_transcript_revision,
            latest_render_id: project.latest_render_id,
            created_at: format_time(project.created_at),
            updated_at: format_time(project.updated_at),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectPageResponse {
    pub items: Vec<ProjectResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VideoSettingsResponse {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub background_fit: BackgroundFit,
}

impl From<VideoSettings> for VideoSettingsResponse {
    fn from(settings: VideoSettings) -> Self {
        Self {
            width: settings.width,
            height: settings.height,
            fps: settings.fps,
            background_fit: settings.background_fit,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssetResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub original_filename: Option<String>,
    pub media_type: String,
    pub bytes: i64,
    pub sha256: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartTranscriptionRequest {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub initial_prompt: Option<String>,
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
}

fn default_language() -> String {
    "auto".to_owned()
}

fn default_model() -> String {
    "configured-default".to_owned()
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct JobAcceptedResponse {
    pub job: JobResponse,
    pub job_url: String,
    pub events_url: String,
}

#[derive(Debug, Serialize)]
pub struct TranscriptRevisionResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub revision: i32,
    pub source: String,
    pub language: String,
    pub duration_ms: i64,
    pub cues: Vec<TranscriptCue>,
    pub transcriber: Option<TranscriberMetadata>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceTranscriptRequest {
    pub language: String,
    pub duration_ms: i64,
    pub cues: Vec<TranscriptCue>,
}

impl From<TranscriptRevision> for TranscriptRevisionResponse {
    fn from(transcript: TranscriptRevision) -> Self {
        Self {
            id: transcript.id,
            project_id: transcript.project_id,
            revision: transcript.revision,
            source: transcript.source,
            language: transcript.language,
            duration_ms: transcript.duration_ms,
            cues: transcript.cues,
            transcriber: transcript.transcriber,
            created_at: format_time(transcript.created_at),
        }
    }
}

impl From<Asset> for AssetResponse {
    fn from(asset: Asset) -> Self {
        Self {
            id: asset.id,
            project_id: asset.project_id,
            kind: asset.kind.to_string(),
            original_filename: asset.original_filename,
            media_type: asset.media_type,
            bytes: asset.bytes,
            sha256: asset.sha256,
            duration_ms: asset.duration_ms,
            width: asset.width,
            height: asset.height,
            content_url: Some(format!("/api/v1/artifacts/{}/content", asset.id)),
            created_at: format_time(asset.created_at),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: Uuid,
    pub r#type: String,
    pub status: String,
    pub phase: String,
    pub progress: f64,
    pub attempt: i32,
    pub max_attempts: i32,
    pub cancellable: bool,
    pub result: Option<Value>,
    pub error: Option<lyrit_domain::JobError>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl From<Job> for JobResponse {
    fn from(job: Job) -> Self {
        Self {
            id: job.id,
            r#type: job.kind,
            status: job.status.to_string(),
            phase: job.phase,
            progress: job.progress,
            attempt: job.attempt,
            max_attempts: job.max_attempts,
            cancellable: !job.status.is_terminal(),
            result: job.result,
            error: job.error,
            created_at: format_time(job.created_at),
            started_at: job.started_at.map(format_time),
            finished_at: job.finished_at.map(format_time),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JobEventResponse {
    pub id: i64,
    pub job_id: Uuid,
    pub sequence: i32,
    pub status: String,
    pub phase: String,
    pub progress: f64,
    pub message: Option<String>,
    pub occurred_at: String,
}

impl From<JobEvent> for JobEventResponse {
    fn from(event: JobEvent) -> Self {
        Self {
            id: event.id,
            job_id: event.job_id,
            sequence: event.sequence,
            status: event.status.to_string(),
            phase: event.phase,
            progress: event.progress,
            message: event.message,
            occurred_at: format_time(event.occurred_at),
        }
    }
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .expect("OffsetDateTime should format as RFC 3339")
}
