mod error;
mod routes;
mod state;

use std::{env, error::Error, net::SocketAddr};

use lyrit_application::{JobService, ProjectService};
use lyrit_persistence::{PgJobRepository, PgProjectRepository};
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

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("../../db/migrations").run(&pool).await?;

    let jobs = JobService::new(PgJobRepository::new(pool.clone()));
    let projects = ProjectService::new(PgProjectRepository::new(pool));
    let state = AppState {
        jobs,
        projects,
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

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}
