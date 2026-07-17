use async_trait::async_trait;
use lyrit_domain::{Job, JobError, JobEvent, Project, VideoSettings};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("resource not found")]
    NotFound,
    #[error("invalid request: {0}")]
    Validation(String),
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
    use lyrit_domain::{BackgroundFit, VideoSettings};

    use super::{ApplicationError, validate_name, validate_video_settings};

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
}
