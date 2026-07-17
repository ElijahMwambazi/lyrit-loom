use async_trait::async_trait;
use lyrit_domain::{Job, JobError, JobEvent};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("resource not found")]
    NotFound,
    #[error("repository failure: {0}")]
    Repository(String),
    #[error("invalid persisted data: {0}")]
    InvalidData(String),
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
