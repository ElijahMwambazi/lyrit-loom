use std::{env, error::Error, time::Duration};

use lyrit_application::JobService;
use lyrit_domain::{Job, JobError};
use lyrit_persistence::PgJobRepository;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Clone)]
struct WorkerConfig {
    worker_id: String,
    poll_interval: Duration,
    lease_seconds: i32,
    run_once: bool,
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
    let jobs = JobService::new(PgJobRepository::new(pool));
    jobs.readiness().await?;

    let config = WorkerConfig {
        worker_id: env::var("WORKER_ID").unwrap_or_else(|_| format!("worker-{}", Uuid::new_v4())),
        poll_interval: Duration::from_millis(env_u64("WORKER_POLL_MS", 750)),
        lease_seconds: env_i32("WORKER_LEASE_SECONDS", 30),
        run_once: env_flag("WORKER_RUN_ONCE", false),
    };

    info!(worker_id = %config.worker_id, "worker ready");
    run(jobs, config).await;
    Ok(())
}

async fn run(jobs: JobService<PgJobRepository>, config: WorkerConfig) {
    let supported_kinds = vec!["system_probe".to_owned()];

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("worker shutdown signal received");
                break;
            }
            claim = jobs.claim_next(&config.worker_id, &supported_kinds, config.lease_seconds) => {
                match claim {
                    Ok(Some(job)) => {
                        let job_id = job.id;
                        info!(%job_id, kind = %job.kind, attempt = job.attempt, "claimed job");
                        if let Err(error) = handle_job(&jobs, &config, job).await {
                            error!(%job_id, %error, "job handler failed");
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
    config: &WorkerConfig,
    job: Job,
) -> Result<(), lyrit_application::ApplicationError> {
    if job.kind != "system_probe" {
        jobs.fail(
            job.id,
            &config.worker_id,
            JobError {
                code: "unsupported_job_kind".to_owned(),
                message: "This scaffold worker supports only system_probe jobs.".to_owned(),
                retryable: false,
            },
        )
        .await?;
        return Ok(());
    }

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
