use std::{
    env,
    error::Error,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use lyrit_application::{ActivateTranscript, JobService, TranscriptService};
use lyrit_domain::{Job, JobError, TimedWord, TranscriberMetadata, TranscriptCue};
use lyrit_persistence::{PgJobRepository, PgTranscriptRepository};
use serde::Deserialize;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tokio::{fs, process::Command, time::timeout};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Clone)]
struct WorkerConfig {
    worker_id: String,
    poll_interval: Duration,
    lease_seconds: i32,
    run_once: bool,
    artifact_root: PathBuf,
    workspace_root: PathBuf,
    ffmpeg_path: String,
    python_path: String,
    supported_kinds: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://lyrit:lyrit@localhost:5432/lyrit".to_owned());
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    let jobs = JobService::new(PgJobRepository::new(pool.clone()));
    let transcripts = TranscriptService::new(PgTranscriptRepository::new(pool));
    jobs.readiness().await?;

    let config = WorkerConfig {
        worker_id: env::var("WORKER_ID").unwrap_or_else(|_| format!("worker-{}", Uuid::new_v4())),
        poll_interval: Duration::from_millis(env_u64("WORKER_POLL_MS", 750)),
        lease_seconds: env_i32("WORKER_LEASE_SECONDS", 30),
        run_once: env_flag("WORKER_RUN_ONCE", false),
        artifact_root: env::var("ARTIFACT_ROOT")
            .unwrap_or_else(|_| "./artifacts".to_owned())
            .into(),
        workspace_root: env::var("WORKSPACE_ROOT")
            .unwrap_or_else(|_| "./workspaces".to_owned())
            .into(),
        ffmpeg_path: env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".to_owned()),
        python_path: env::var("PYTHON_PATH").unwrap_or_else(|_| "python3".to_owned()),
        supported_kinds: env::var("WORKER_QUEUES")
            .unwrap_or_else(|_| "system_probe,transcribe".to_owned())
            .split(',')
            .map(str::trim)
            .filter(|kind| matches!(*kind, "system_probe" | "transcribe"))
            .map(str::to_owned)
            .collect(),
    };

    if config.supported_kinds.is_empty() {
        return Err("WORKER_QUEUES must include system_probe or transcribe".into());
    }

    info!(worker_id = %config.worker_id, "worker ready");
    run(jobs, transcripts, config).await;
    Ok(())
}

async fn run(
    jobs: JobService<PgJobRepository>,
    transcripts: TranscriptService<PgTranscriptRepository>,
    config: WorkerConfig,
) {
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("worker shutdown signal received");
                break;
            }
            claim = jobs.claim_next(&config.worker_id, &config.supported_kinds, config.lease_seconds) => {
                match claim {
                    Ok(Some(job)) => {
                        let job_id = job.id;
                        info!(%job_id, kind = %job.kind, attempt = job.attempt, "claimed job");
                        if let Err(error) = handle_job(&jobs, &transcripts, &config, job).await {
                            error!(%job_id, %error, "job handler failed");
                            let _ = jobs.fail(job_id, &config.worker_id, JobError {
                                code: "transcription_failed".to_owned(),
                                message: "The transcription job could not be completed.".to_owned(),
                                retryable: false,
                            }).await;
                        }
                        if config.run_once {
                            break;
                        }
                    }
                    Ok(None) => {
                        if config.run_once {
                            info!("no queued job found in run-once mode");
                            break;
                        }
                        tokio::time::sleep(config.poll_interval).await;
                    }
                    Err(error) => {
                        warn!(%error, "job claim failed; retrying");
                        tokio::time::sleep(config.poll_interval).await;
                    }
                }
            }
        }
    }
}

async fn handle_job(
    jobs: &JobService<PgJobRepository>,
    transcripts: &TranscriptService<PgTranscriptRepository>,
    config: &WorkerConfig,
    job: Job,
) -> Result<(), lyrit_application::ApplicationError> {
    match job.kind.as_str() {
        "system_probe" => handle_probe(jobs, config, job).await,
        "transcribe" => handle_transcription(jobs, transcripts, config, job).await,
        _ => {
            jobs.fail(
                job.id,
                &config.worker_id,
                JobError {
                    code: "unsupported_job_kind".to_owned(),
                    message: "This worker does not support the queued job kind.".to_owned(),
                    retryable: false,
                },
            )
            .await?;
            Ok(())
        }
    }
}

async fn handle_probe(
    jobs: &JobService<PgJobRepository>,
    config: &WorkerConfig,
    job: Job,
) -> Result<(), lyrit_application::ApplicationError> {
    let phases = [
        ("validating", 0.15, "Validating durable job payload"),
        (
            "checking_infrastructure",
            0.55,
            "Checking worker infrastructure",
        ),
        ("finalizing", 0.9, "Finalizing probe result"),
    ];

    for (phase, progress, message) in phases {
        jobs.progress(
            job.id,
            &config.worker_id,
            phase,
            progress,
            Some(message),
            config.lease_seconds,
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(120)).await;
    }

    jobs.complete(
        job.id,
        &config.worker_id,
        json!({
            "worker_id": config.worker_id,
            "message": "Durable job queue is operational"
        }),
    )
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptionPayload {
    project_id: Uuid,
    audio_asset_id: Uuid,
    source_storage_key: String,
    language: String,
    model: String,
    initial_prompt: Option<String>,
    vad_enabled: bool,
    source_duration_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriberResult {
    contract_version: String,
    language: String,
    language_probability: f64,
    duration_ms: i64,
    model: TranscriberModel,
    segments: Vec<TranscriberSegment>,
    #[serde(rename = "warnings")]
    _warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriberModel {
    engine: String,
    name: String,
    revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriberSegment {
    start_ms: i64,
    end_ms: i64,
    #[serde(rename = "text")]
    _text: String,
    words: Vec<TranscriberWord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriberWord {
    text: String,
    start_ms: i64,
    end_ms: i64,
    confidence: f64,
}

async fn handle_transcription(
    jobs: &JobService<PgJobRepository>,
    transcripts: &TranscriptService<PgTranscriptRepository>,
    config: &WorkerConfig,
    job: Job,
) -> Result<(), lyrit_application::ApplicationError> {
    let payload: TranscriptionPayload = serde_json::from_value(job.payload.clone())
        .map_err(|error| lyrit_application::ApplicationError::InvalidData(error.to_string()))?;
    let source_path = resolve_artifact(&config.artifact_root, &payload.source_storage_key)?;
    let workspace = config.workspace_root.join(job.id.to_string());
    fs::create_dir_all(&workspace)
        .await
        .map_err(worker_artifact_error)?;
    let result = run_transcription(
        jobs,
        transcripts,
        config,
        &job,
        &payload,
        &workspace,
        &source_path,
    )
    .await;
    let _ = fs::remove_dir_all(&workspace).await;
    result
}

async fn run_transcription(
    jobs: &JobService<PgJobRepository>,
    transcripts: &TranscriptService<PgTranscriptRepository>,
    config: &WorkerConfig,
    job: &Job,
    payload: &TranscriptionPayload,
    workspace: &Path,
    source_path: &Path,
) -> Result<(), lyrit_application::ApplicationError> {
    jobs.progress(
        job.id,
        &config.worker_id,
        "normalizing",
        0.15,
        Some("Normalizing audio"),
        config.lease_seconds,
    )
    .await?;
    let normalized_path = workspace.join("normalized.wav");
    let mut ffmpeg = Command::new(&config.ffmpeg_path);
    ffmpeg
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source_path)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(&normalized_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let ffmpeg_output = timeout(Duration::from_secs(600), ffmpeg.output())
        .await
        .map_err(|_| {
            lyrit_application::ApplicationError::MediaInspection(
                "audio normalization timed out".to_owned(),
            )
        })?
        .map_err(|error| lyrit_application::ApplicationError::MediaInspection(error.to_string()))?;
    if !ffmpeg_output.status.success() {
        return Err(lyrit_application::ApplicationError::MediaInspection(
            "audio normalization failed".to_owned(),
        ));
    }

    jobs.progress(
        job.id,
        &config.worker_id,
        "transcribing",
        0.45,
        Some("Running transcriber"),
        config.lease_seconds,
    )
    .await?;
    let output_path = workspace.join("transcript.json");
    let request_path = workspace.join("request.json");
    let request = json!({
        "contract_version": "1",
        "request_id": job.id,
        "input_path": normalized_path,
        "output_path": output_path,
        "language": payload.language,
        "model": payload.model,
        "word_timestamps": true,
        "vad": { "enabled": payload.vad_enabled },
        "initial_prompt": payload.initial_prompt
    });
    fs::write(
        &request_path,
        serde_json::to_vec(&request).map_err(worker_data_error)?,
    )
    .await
    .map_err(worker_artifact_error)?;
    let mut transcriber = Command::new(&config.python_path);
    transcriber
        .args(["-m", "lyrit_transcriber", "--request"])
        .arg(&request_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let transcriber_output = timeout(Duration::from_secs(120), transcriber.output())
        .await
        .map_err(|_| {
            lyrit_application::ApplicationError::MediaInspection("transcriber timed out".to_owned())
        })?
        .map_err(|error| lyrit_application::ApplicationError::MediaInspection(error.to_string()))?;
    if !transcriber_output.status.success() {
        return Err(lyrit_application::ApplicationError::MediaInspection(
            "transcriber process failed".to_owned(),
        ));
    }

    jobs.progress(
        job.id,
        &config.worker_id,
        "post_processing",
        0.78,
        Some("Validating word timestamps"),
        config.lease_seconds,
    )
    .await?;
    let raw_result = fs::read(&output_path)
        .await
        .map_err(worker_artifact_error)?;
    let result: TranscriberResult =
        serde_json::from_slice(&raw_result).map_err(worker_data_error)?;
    if result.contract_version != "1" {
        return Err(lyrit_application::ApplicationError::InvalidData(
            "unsupported transcriber result version".to_owned(),
        ));
    }
    if result.duration_ms > payload.source_duration_ms.saturating_add(1_000) {
        return Err(lyrit_application::ApplicationError::InvalidData(
            "transcriber duration exceeds the active audio duration".to_owned(),
        ));
    }
    let cues = result
        .segments
        .into_iter()
        .map(|segment| TranscriptCue {
            id: Uuid::new_v4(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            words: segment
                .words
                .into_iter()
                .map(|word| TimedWord {
                    id: Uuid::new_v4(),
                    text: word.text,
                    start_ms: word.start_ms,
                    end_ms: word.end_ms,
                    confidence: Some(word.confidence),
                })
                .collect(),
        })
        .collect();
    let transcript = transcripts
        .activate(ActivateTranscript {
            job_id: job.id,
            project_id: payload.project_id,
            audio_asset_id: payload.audio_asset_id,
            language: result.language,
            duration_ms: result.duration_ms,
            cues,
            transcriber: TranscriberMetadata {
                engine: result.model.engine,
                model: result.model.name,
                model_revision: result.model.revision,
                language_probability: result.language_probability,
            },
        })
        .await?;
    jobs.progress(
        job.id,
        &config.worker_id,
        "finalizing",
        0.95,
        Some("Activating transcript revision"),
        config.lease_seconds,
    )
    .await?;
    jobs.complete(
        job.id,
        &config.worker_id,
        json!({
            "project_id": transcript.project_id,
            "transcript_id": transcript.id,
            "transcript_revision": transcript.revision
        }),
    )
    .await?;
    Ok(())
}

fn resolve_artifact(
    root: &Path,
    storage_key: &str,
) -> Result<PathBuf, lyrit_application::ApplicationError> {
    let key = Path::new(storage_key);
    if storage_key.is_empty()
        || key.is_absolute()
        || key
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(lyrit_application::ApplicationError::Artifact(
            "job payload contains an unsafe artifact key".to_owned(),
        ));
    }
    Ok(root.join(key))
}

fn worker_artifact_error(error: std::io::Error) -> lyrit_application::ApplicationError {
    lyrit_application::ApplicationError::Artifact(error.to_string())
}

fn worker_data_error(error: serde_json::Error) -> lyrit_application::ApplicationError {
    lyrit_application::ApplicationError::InvalidData(error.to_string())
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("lyrit_worker=debug"));
    let format = env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".to_owned());
    if format == "json" {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .compact()
            .init();
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_i32(name: &str, default: i32) -> i32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
