use std::str::FromStr;

use async_trait::async_trait;
use lyrit_application::{ApplicationError, JobRepository};
use lyrit_domain::{Job, JobError, JobEvent, JobStatus};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

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
