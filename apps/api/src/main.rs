mod error;
mod routes;
mod state;

use std::{env, error::Error, net::SocketAddr};

use lyrit_application::{AssetService, JobService, ProjectService};
use lyrit_media::{FfprobeMediaInspector, LocalArtifactStore};
use lyrit_persistence::{PgAssetRepository, PgJobRepository, PgProjectRepository};
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://lyrit:lyrit@localhost:5432/lyrit".to_owned());
    let bind_address = env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let enable_dev_routes = env_flag("ENABLE_DEV_ROUTES", true);
    let artifact_store = env::var("ARTIFACT_STORE").unwrap_or_else(|_| "local".to_owned());
    if artifact_store != "local" {
        return Err(format!("unsupported ARTIFACT_STORE: {artifact_store}").into());
    }
    let artifact_root = env::var("ARTIFACT_ROOT").unwrap_or_else(|_| "./artifacts".to_owned());
    let ffprobe_path = env::var("FFPROBE_PATH").unwrap_or_else(|_| "ffprobe".to_owned());
    let max_upload_bytes = env_number("MAX_UPLOAD_BYTES", 536_870_912_i64)?;
    let max_audio_duration_ms = env_number("MAX_AUDIO_DURATION_MS", 900_000_i64)?;
    if max_upload_bytes <= 0 || max_audio_duration_ms <= 0 {
        return Err("upload limits must be positive".into());
    }

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("../../db/migrations").run(&pool).await?;

    let jobs = JobService::new(PgJobRepository::new(pool.clone()));
    let project_repository = PgProjectRepository::new(pool.clone());
    let projects = ProjectService::new(project_repository.clone());
    let assets = AssetService::new(
        PgAssetRepository::new(pool),
        project_repository,
        LocalArtifactStore::new(&artifact_root),
        FfprobeMediaInspector::new(&artifact_root, ffprobe_path),
        max_upload_bytes,
        max_audio_duration_ms,
    );
    let state = AppState {
        jobs,
        projects,
        assets,
        max_upload_bytes: usize::try_from(max_upload_bytes)?,
        enable_dev_routes,
    };
    let app = routes::router(state);
    let address: SocketAddr = bind_address.parse()?;
    let listener = TcpListener::bind(address).await?;

    info!(%address, enable_dev_routes, "Lyrit Loom API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("lyrit_api=debug,tower_http=info"));
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

fn env_number<T>(name: &str, default: T) -> Result<T, Box<dyn Error>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
{
    match env::var(name) {
        Ok(value) => Ok(value.parse()?),
        Err(_) => Ok(default),
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}
