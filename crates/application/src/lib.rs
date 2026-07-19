use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use lyrit_domain::{
    Asset, AssetKind, Job, JobError, JobEvent, Project, TranscriberMetadata, TranscriptCue,
    TranscriptRevision, VideoSettings,
};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("resource not found")]
    NotFound,
    #[error("invalid request: {0}")]
    Validation(String),
    #[error("resource state conflict: {0}")]
    Conflict(String),
    #[error("upload exceeds the configured size limit")]
    PayloadTooLarge,
    #[error("unsupported media: {0}")]
    UnsupportedMedia(String),
    #[error("artifact operation failed: {0}")]
    Artifact(String),
    #[error("media inspection failed: {0}")]
    MediaInspection(String),
    #[error("repository failure: {0}")]
    Repository(String),
    #[error("invalid persisted data: {0}")]
    InvalidData(String),
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub video_settings: VideoSettings,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectChanges {
    pub name: Option<String>,
    pub video_settings: Option<VideoSettings>,
}

#[derive(Debug, Clone)]
pub struct ProjectPage {
    pub items: Vec<Project>,
    pub next_cursor: Option<String>,
}

pub type ByteStream<'a> = Pin<Box<dyn Stream<Item = Result<Bytes, ApplicationError>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub struct StoredObject {
    pub storage_key: String,
    pub bytes: i64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct MediaFacts {
    pub media_type: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub tool_metadata: Value,
}

#[derive(Debug, Clone)]
pub struct NewAsset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: AssetKind,
    pub storage_key: String,
    pub original_filename: Option<String>,
    pub media_type: String,
    pub bytes: i64,
    pub sha256: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub tool_metadata: Value,
}

#[derive(Debug, Clone)]
pub struct StartTranscription {
    pub owner_id: Uuid,
    pub project_id: Uuid,
    pub idempotency_key: String,
    pub language: String,
    pub model: String,
    pub initial_prompt: Option<String>,
    pub vad_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ActivateTranscript {
    pub job_id: Uuid,
    pub project_id: Uuid,
    pub audio_asset_id: Uuid,
    pub language: String,
    pub duration_ms: i64,
    pub cues: Vec<TranscriptCue>,
    pub transcriber: TranscriberMetadata,
}

#[async_trait]
pub trait TranscriptRepository: Clone + Send + Sync + 'static {
    async fn enqueue(&self, request: StartTranscription) -> Result<Option<Job>, ApplicationError>;
    async fn get_active(
        &self,
        owner_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<TranscriptRevision>, ApplicationError>;
    async fn activate(
        &self,
        transcript: ActivateTranscript,
    ) -> Result<TranscriptRevision, ApplicationError>;
}

#[derive(Clone)]
pub struct TranscriptService<R> {
    repository: R,
}

impl<R> TranscriptService<R>
where
    R: TranscriptRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn start(&self, mut request: StartTranscription) -> Result<Job, ApplicationError> {
        request.idempotency_key = request.idempotency_key.trim().to_owned();
        request.language = request.language.trim().to_owned();
        request.model = request.model.trim().to_owned();
        if request.idempotency_key.is_empty() || request.idempotency_key.len() > 128 {
            return Err(ApplicationError::Validation(
                "Idempotency-Key must contain between 1 and 128 characters".to_owned(),
            ));
        }
        if request.language.is_empty() || request.language.len() > 64 {
            return Err(ApplicationError::Validation(
                "language must contain between 1 and 64 characters".to_owned(),
            ));
        }
        if request.model != "configured-default" {
            return Err(ApplicationError::Validation(
                "model must be the configured-default profile".to_owned(),
            ));
        }
        if request
            .initial_prompt
            .as_ref()
            .is_some_and(|value| value.chars().count() > 1000)
        {
            return Err(ApplicationError::Validation(
                "initial_prompt must not exceed 1000 characters".to_owned(),
            ));
        }
        self.repository
            .enqueue(request)
            .await?
            .ok_or(ApplicationError::Conflict(
                "project does not have an active audio asset".to_owned(),
            ))
    }

    pub async fn get_active(
        &self,
        owner_id: Uuid,
        project_id: Uuid,
    ) -> Result<TranscriptRevision, ApplicationError> {
        self.repository
            .get_active(owner_id, project_id)
            .await?
            .ok_or(ApplicationError::NotFound)
    }

    pub async fn activate(
        &self,
        transcript: ActivateTranscript,
    ) -> Result<TranscriptRevision, ApplicationError> {
        validate_transcript(&transcript)?;
        self.repository.activate(transcript).await
    }
}

fn validate_transcript(transcript: &ActivateTranscript) -> Result<(), ApplicationError> {
    if transcript.duration_ms <= 0
        || transcript.cues.is_empty()
        || transcript.language.trim().is_empty()
        || transcript.language.len() > 64
    {
        return Err(ApplicationError::Validation(
            "transcript language, duration, and cues must be present".to_owned(),
        ));
    }
    if !matches!(
        transcript.transcriber.engine.as_str(),
        "fake" | "faster-whisper"
    ) || transcript.transcriber.model.trim().is_empty()
        || transcript.transcriber.model_revision.trim().is_empty()
        || !(0.0..=1.0).contains(&transcript.transcriber.language_probability)
    {
        return Err(ApplicationError::Validation(
            "transcriber metadata does not satisfy the contract".to_owned(),
        ));
    }
    let mut previous_end = 0;
    for cue in &transcript.cues {
        if cue.words.is_empty()
            || cue.start_ms < previous_end
            || cue.end_ms <= cue.start_ms
            || cue.end_ms > transcript.duration_ms
        {
            return Err(ApplicationError::Validation(
                "transcript cues must be ordered, non-empty, and within duration".to_owned(),
            ));
        }
        let mut word_end = cue.start_ms;
        for word in &cue.words {
            if word.text.trim().is_empty()
                || word.start_ms < word_end
                || word.end_ms <= word.start_ms
                || word.start_ms < cue.start_ms
                || word.end_ms > cue.end_ms
                || word
                    .confidence
                    .is_some_and(|value| !(0.0..=1.0).contains(&value))
            {
                return Err(ApplicationError::Validation(
                    "transcript words must be ordered and contained by their cue".to_owned(),
                ));
            }
            word_end = word.end_ms;
        }
        previous_end = cue.end_ms;
    }
    Ok(())
}

#[async_trait]
pub trait AssetRepository: Clone + Send + Sync + 'static {
    async fn get(&self, id: Uuid) -> Result<Option<Asset>, ApplicationError>;
    async fn activate(
        &self,
        owner_id: Uuid,
        asset: NewAsset,
    ) -> Result<Option<Asset>, ApplicationError>;
}

#[async_trait]
pub trait ArtifactStore: Clone + Send + Sync + 'static {
    async fn put(
        &self,
        storage_key: &str,
        body: ByteStream<'_>,
        max_bytes: i64,
    ) -> Result<StoredObject, ApplicationError>;
    async fn delete(&self, storage_key: &str) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait MediaInspector: Clone + Send + Sync + 'static {
    async fn inspect(
        &self,
        storage_key: &str,
        kind: AssetKind,
    ) -> Result<MediaFacts, ApplicationError>;
}

pub struct UploadAsset<'a> {
    pub project_id: Uuid,
    pub kind: AssetKind,
    pub original_filename: Option<String>,
    pub declared_media_type: Option<String>,
    pub body: ByteStream<'a>,
}

#[derive(Clone)]
pub struct AssetService<A, P, S, M> {
    assets: A,
    projects: P,
    store: S,
    inspector: M,
    max_upload_bytes: i64,
    max_audio_duration_ms: i64,
}

impl<A, P, S, M> AssetService<A, P, S, M>
where
    A: AssetRepository,
    P: ProjectRepository,
    S: ArtifactStore,
    M: MediaInspector,
{
    pub fn new(
        assets: A,
        projects: P,
        store: S,
        inspector: M,
        max_upload_bytes: i64,
        max_audio_duration_ms: i64,
    ) -> Self {
        Self {
            assets,
            projects,
            store,
            inspector,
            max_upload_bytes,
            max_audio_duration_ms,
        }
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<Asset>, ApplicationError> {
        self.assets.get(id).await
    }

    pub async fn upload(
        &self,
        owner_id: Uuid,
        upload: UploadAsset<'_>,
    ) -> Result<Asset, ApplicationError> {
        if !upload.kind.is_source() {
            return Err(ApplicationError::Validation(
                "only audio and background source assets may be uploaded".to_owned(),
            ));
        }
        self.projects
            .get(owner_id, upload.project_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        validate_declared_media_type(upload.kind, upload.declared_media_type.as_deref())?;
        let original_filename = validate_filename(upload.original_filename)?;
        let asset_id = Uuid::new_v4();
        let storage_key = format!("projects/{}/assets/{}/source", upload.project_id, asset_id);
        let stored = self
            .store
            .put(&storage_key, upload.body, self.max_upload_bytes)
            .await?;

        let facts = match self.inspector.inspect(&storage_key, upload.kind).await {
            Ok(facts) => facts,
            Err(error) => {
                let _ = self.store.delete(&storage_key).await;
                return Err(error);
            }
        };
        if upload.kind == AssetKind::Audio
            && facts
                .duration_ms
                .is_some_and(|duration| duration > self.max_audio_duration_ms)
        {
            let _ = self.store.delete(&storage_key).await;
            return Err(ApplicationError::UnsupportedMedia(format!(
                "audio duration exceeds {} milliseconds",
                self.max_audio_duration_ms
            )));
        }

        let asset = NewAsset {
            id: asset_id,
            project_id: upload.project_id,
            kind: upload.kind,
            storage_key: stored.storage_key.clone(),
            original_filename,
            media_type: facts.media_type,
            bytes: stored.bytes,
            sha256: stored.sha256,
            duration_ms: facts.duration_ms,
            width: facts.width,
            height: facts.height,
            tool_metadata: facts.tool_metadata,
        };
        match self.assets.activate(owner_id, asset).await {
            Ok(Some(asset)) => Ok(asset),
            Ok(None) => {
                let _ = self.store.delete(&stored.storage_key).await;
                Err(ApplicationError::NotFound)
            }
            Err(error) => {
                let _ = self.store.delete(&stored.storage_key).await;
                Err(error)
            }
        }
    }
}

fn validate_declared_media_type(
    kind: AssetKind,
    declared_media_type: Option<&str>,
) -> Result<(), ApplicationError> {
    let Some(media_type) = declared_media_type else {
        return Ok(());
    };
    let expected = match kind {
        AssetKind::Audio => "audio/",
        AssetKind::Background => "image/",
        _ => return Ok(()),
    };
    if media_type.starts_with(expected) || media_type == "application/octet-stream" {
        Ok(())
    } else {
        Err(ApplicationError::UnsupportedMedia(format!(
            "expected {expected} media but received {media_type}"
        )))
    }
}

fn validate_filename(filename: Option<String>) -> Result<Option<String>, ApplicationError> {
    filename
        .map(|filename| {
            let filename = filename
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            if filename.is_empty()
                || filename.chars().count() > 255
                || filename.chars().any(char::is_control)
            {
                Err(ApplicationError::Validation(
                    "original filename must contain between 1 and 255 safe characters".to_owned(),
                ))
            } else {
                Ok(filename)
            }
        })
        .transpose()
}

#[async_trait]
pub trait ProjectRepository: Clone + Send + Sync + 'static {
    async fn create(&self, project: NewProject) -> Result<Project, ApplicationError>;
    async fn get(&self, owner_id: Uuid, id: Uuid) -> Result<Option<Project>, ApplicationError>;
    async fn list(
        &self,
        owner_id: Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Project>, ApplicationError>;
    async fn update(
        &self,
        owner_id: Uuid,
        id: Uuid,
        changes: ProjectChanges,
    ) -> Result<Option<Project>, ApplicationError>;
}

#[derive(Clone)]
pub struct ProjectService<R> {
    repository: R,
}

impl<R> ProjectService<R>
where
    R: ProjectRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn create(
        &self,
        owner_id: Uuid,
        name: String,
        video_settings: Option<VideoSettings>,
    ) -> Result<Project, ApplicationError> {
        let name = validate_name(name)?;
        let video_settings = video_settings.unwrap_or_default();
        validate_video_settings(video_settings)?;
        self.repository
            .create(NewProject {
                id: Uuid::new_v4(),
                owner_id,
                name,
                video_settings,
            })
            .await
    }

    pub async fn get(&self, owner_id: Uuid, id: Uuid) -> Result<Project, ApplicationError> {
        self.repository
            .get(owner_id, id)
            .await?
            .ok_or(ApplicationError::NotFound)
    }

    pub async fn list(
        &self,
        owner_id: Uuid,
        cursor: Option<String>,
        limit: i64,
    ) -> Result<ProjectPage, ApplicationError> {
        if !(1..=100).contains(&limit) {
            return Err(ApplicationError::Validation(
                "limit must be between 1 and 100".to_owned(),
            ));
        }
        let offset = cursor
            .as_deref()
            .unwrap_or("0")
            .parse::<i64>()
            .map_err(|_| ApplicationError::Validation("invalid project cursor".to_owned()))?;
        if offset < 0 {
            return Err(ApplicationError::Validation(
                "invalid project cursor".to_owned(),
            ));
        }

        let mut items = self.repository.list(owner_id, offset, limit + 1).await?;
        let next_cursor = (items.len() as i64 > limit).then(|| (offset + limit).to_string());
        items.truncate(limit as usize);
        Ok(ProjectPage { items, next_cursor })
    }

    pub async fn update(
        &self,
        owner_id: Uuid,
        id: Uuid,
        mut changes: ProjectChanges,
    ) -> Result<Project, ApplicationError> {
        if changes.name.is_none() && changes.video_settings.is_none() {
            return Err(ApplicationError::Validation(
                "at least one project field must be provided".to_owned(),
            ));
        }
        if let Some(name) = changes.name.take() {
            changes.name = Some(validate_name(name)?);
        }
        if let Some(settings) = changes.video_settings {
            validate_video_settings(settings)?;
        }

        self.repository
            .update(owner_id, id, changes)
            .await?
            .ok_or(ApplicationError::NotFound)
    }
}

fn validate_name(name: String) -> Result<String, ApplicationError> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(ApplicationError::Validation(
            "project name must contain between 1 and 120 characters".to_owned(),
        ));
    }
    Ok(name)
}

fn validate_video_settings(settings: VideoSettings) -> Result<(), ApplicationError> {
    if !(320..=3840).contains(&settings.width) || !(320..=3840).contains(&settings.height) {
        return Err(ApplicationError::Validation(
            "video dimensions must be between 320 and 3840 pixels".to_owned(),
        ));
    }
    if ![24, 25, 30, 50, 60].contains(&settings.fps) {
        return Err(ApplicationError::Validation(
            "video fps must be one of 24, 25, 30, 50, or 60".to_owned(),
        ));
    }
    Ok(())
}

#[async_trait]
pub trait JobRepository: Clone + Send + Sync + 'static {
    async fn ping(&self) -> Result<(), ApplicationError>;
    async fn enqueue_probe(&self, payload: Value) -> Result<Job, ApplicationError>;
    async fn get(&self, id: Uuid) -> Result<Option<Job>, ApplicationError>;
    async fn claim_next(
        &self,
        worker_id: &str,
        supported_kinds: &[String],
        lease_seconds: i32,
    ) -> Result<Option<Job>, ApplicationError>;
    async fn update_progress(
        &self,
        id: Uuid,
        worker_id: &str,
        phase: &str,
        progress: f64,
        message: Option<&str>,
        lease_seconds: i32,
    ) -> Result<Job, ApplicationError>;
    async fn complete(
        &self,
        id: Uuid,
        worker_id: &str,
        result: Value,
    ) -> Result<Job, ApplicationError>;
    async fn fail(
        &self,
        id: Uuid,
        worker_id: &str,
        error: JobError,
    ) -> Result<Job, ApplicationError>;
    async fn events_after(
        &self,
        id: Uuid,
        after_event_id: i64,
        limit: i64,
    ) -> Result<Vec<JobEvent>, ApplicationError>;
}

#[derive(Clone)]
pub struct JobService<R> {
    repository: R,
}

impl<R> JobService<R>
where
    R: JobRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn readiness(&self) -> Result<(), ApplicationError> {
        self.repository.ping().await
    }

    pub async fn enqueue_probe(&self, requested_by: &str) -> Result<Job, ApplicationError> {
        self.repository
            .enqueue_probe(json!({ "requested_by": requested_by }))
            .await
    }

    pub async fn get(&self, id: Uuid) -> Result<Job, ApplicationError> {
        self.repository
            .get(id)
            .await?
            .ok_or(ApplicationError::NotFound)
    }

    pub async fn claim_next(
        &self,
        worker_id: &str,
        supported_kinds: &[String],
        lease_seconds: i32,
    ) -> Result<Option<Job>, ApplicationError> {
        self.repository
            .claim_next(worker_id, supported_kinds, lease_seconds)
            .await
    }

    pub async fn progress(
        &self,
        job_id: Uuid,
        worker_id: &str,
        phase: &str,
        progress: f64,
        message: Option<&str>,
        lease_seconds: i32,
    ) -> Result<Job, ApplicationError> {
        self.repository
            .update_progress(
                job_id,
                worker_id,
                phase,
                progress.clamp(0.0, 1.0),
                message,
                lease_seconds,
            )
            .await
    }

    pub async fn complete(
        &self,
        job_id: Uuid,
        worker_id: &str,
        result: Value,
    ) -> Result<Job, ApplicationError> {
        self.repository.complete(job_id, worker_id, result).await
    }

    pub async fn fail(
        &self,
        job_id: Uuid,
        worker_id: &str,
        error: JobError,
    ) -> Result<Job, ApplicationError> {
        self.repository.fail(job_id, worker_id, error).await
    }

    pub async fn events_after(
        &self,
        job_id: Uuid,
        after_event_id: i64,
    ) -> Result<Vec<JobEvent>, ApplicationError> {
        self.repository
            .events_after(job_id, after_event_id, 100)
            .await
    }
}

#[cfg(test)]
mod tests {
    use lyrit_domain::{
        AssetKind, BackgroundFit, TimedWord, TranscriberMetadata, TranscriptCue, VideoSettings,
    };
    use uuid::Uuid;

    use super::{
        ActivateTranscript, ApplicationError, validate_declared_media_type, validate_filename,
        validate_name, validate_transcript, validate_video_settings,
    };

    #[test]
    fn project_names_are_trimmed_and_bounded() {
        assert_eq!(
            validate_name("  Midnight chorus  ".to_owned()).unwrap(),
            "Midnight chorus"
        );
        assert!(matches!(
            validate_name("   ".to_owned()),
            Err(ApplicationError::Validation(_))
        ));
        assert!(matches!(
            validate_name("a".repeat(121)),
            Err(ApplicationError::Validation(_))
        ));
    }

    #[test]
    fn video_settings_enforce_supported_render_bounds() {
        assert!(validate_video_settings(VideoSettings::default()).is_ok());
        assert!(
            validate_video_settings(VideoSettings {
                width: 1920,
                height: 1080,
                fps: 29,
                background_fit: BackgroundFit::Cover,
            })
            .is_err()
        );
        assert!(
            validate_video_settings(VideoSettings {
                width: 200,
                ..VideoSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn upload_metadata_is_safely_normalized() {
        assert_eq!(
            validate_filename(Some("../../music/demo.mp3".to_owned())).unwrap(),
            Some("demo.mp3".to_owned())
        );
        assert!(validate_filename(Some("bad\nname.mp3".to_owned())).is_err());
        assert!(validate_declared_media_type(AssetKind::Audio, Some("audio/mpeg")).is_ok());
        assert!(matches!(
            validate_declared_media_type(AssetKind::Background, Some("audio/mpeg")),
            Err(ApplicationError::UnsupportedMedia(_))
        ));
    }

    #[test]
    fn transcript_timeline_rejects_unordered_words() {
        let mut transcript = valid_transcript();
        transcript.cues[0].words[1].start_ms = 400;
        assert!(matches!(
            validate_transcript(&transcript),
            Err(ApplicationError::Validation(_))
        ));
    }

    #[test]
    fn transcript_timeline_accepts_ordered_words() {
        assert!(validate_transcript(&valid_transcript()).is_ok());
    }

    fn valid_transcript() -> ActivateTranscript {
        ActivateTranscript {
            job_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            audio_asset_id: Uuid::new_v4(),
            language: "en".to_owned(),
            duration_ms: 2_000,
            cues: vec![TranscriptCue {
                id: Uuid::new_v4(),
                start_ms: 100,
                end_ms: 1_500,
                words: vec![
                    TimedWord {
                        id: Uuid::new_v4(),
                        text: "Weave".to_owned(),
                        start_ms: 100,
                        end_ms: 500,
                        confidence: Some(0.99),
                    },
                    TimedWord {
                        id: Uuid::new_v4(),
                        text: "motion".to_owned(),
                        start_ms: 600,
                        end_ms: 1_500,
                        confidence: Some(0.98),
                    },
                ],
            }],
            transcriber: TranscriberMetadata {
                engine: "fake".to_owned(),
                model: "configured-default".to_owned(),
                model_revision: "test".to_owned(),
                language_probability: 1.0,
            },
        }
    }
}
