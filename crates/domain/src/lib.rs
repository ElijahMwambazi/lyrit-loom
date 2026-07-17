use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for JobStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "cancelling" => Ok(Self::Cancelling),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown job status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub status: JobStatus,
    pub phase: String,
    pub progress: f64,
    pub attempt: i32,
    pub max_attempts: i32,
    pub payload: Value,
    pub result: Option<Value>,
    pub error: Option<JobError>,
    pub lease_owner: Option<String>,
    pub created_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobEvent {
    pub id: i64,
    pub job_id: Uuid,
    pub sequence: i32,
    pub status: JobStatus,
    pub phase: String,
    pub progress: f64,
    pub message: Option<String>,
    pub occurred_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Draft,
    Ready,
    Rendering,
    Completed,
    Failed,
}

impl ProjectStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Rendering => "rendering",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for ProjectStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ProjectStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            "rendering" => Ok(Self::Rendering),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown project status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundFit {
    Cover,
    Contain,
    Stretch,
}

impl BackgroundFit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
            Self::Stretch => "stretch",
        }
    }
}

impl fmt::Display for BackgroundFit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BackgroundFit {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cover" => Ok(Self::Cover),
            "contain" => Ok(Self::Contain),
            "stretch" => Ok(Self::Stretch),
            other => Err(format!("unknown background fit: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoSettings {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub background_fit: BackgroundFit,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            background_fit: BackgroundFit::Cover,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub status: ProjectStatus,
    pub video_settings: VideoSettings,
    pub audio_asset_id: Option<Uuid>,
    pub background_asset_id: Option<Uuid>,
    pub active_transcript_revision: Option<i32>,
    pub latest_render_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_are_explicit() {
        assert!(JobStatus::Succeeded.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
    }

    #[test]
    fn status_round_trip_uses_contract_values() {
        for status in [
            JobStatus::Queued,
            JobStatus::Running,
            JobStatus::Cancelling,
            JobStatus::Succeeded,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            assert_eq!(status.to_string().parse::<JobStatus>().unwrap(), status);
        }
    }

    #[test]
    fn project_contract_enums_round_trip() {
        for status in [
            ProjectStatus::Draft,
            ProjectStatus::Ready,
            ProjectStatus::Rendering,
            ProjectStatus::Completed,
            ProjectStatus::Failed,
        ] {
            assert_eq!(status.to_string().parse::<ProjectStatus>().unwrap(), status);
        }

        for fit in [
            BackgroundFit::Cover,
            BackgroundFit::Contain,
            BackgroundFit::Stretch,
        ] {
            assert_eq!(fit.to_string().parse::<BackgroundFit>().unwrap(), fit);
        }
    }
}
