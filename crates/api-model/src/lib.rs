use lyrit_domain::{Job, JobEvent};
use serde::Serialize;
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
