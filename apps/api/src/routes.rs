use std::{convert::Infallible, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use lyrit_api_model::{
    CreateProjectRequest, HealthResponse, JobEventResponse, JobResponse, ProjectPageResponse,
    ProjectResponse, ReadinessCheck, ReadinessResponse, UpdateProjectRequest,
};
use lyrit_application::ProjectChanges;
use serde::Deserialize;
use tokio::time::MissedTickBehavior;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

const LOCAL_OWNER_ID: Uuid = Uuid::from_u128(1);

pub fn router(state: AppState) -> Router {
    let mut api = Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{project_id}",
            get(get_project).patch(update_project),
        )
        .route("/jobs/{job_id}", get(get_job))
        .route("/jobs/{job_id}/events", get(stream_job_events));

    if state.enable_dev_routes {
        api = api.route("/internal/dev/jobs/probe", post(enqueue_probe));
    }

    let request_id = HeaderName::from_static("x-request-id");
    Router::new()
        .nest("/api/v1", api)
        .with_state(state)
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
        .layer(TraceLayer::new_for_http())
}

async fn create_project(
    State(state): State<AppState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<(StatusCode, [(HeaderName, String); 1], Json<ProjectResponse>), ApiError> {
    let project = state
        .projects
        .create(
            LOCAL_OWNER_ID,
            request.name,
            request.video_settings.map(Into::into),
        )
        .await?;
    let location = format!("/api/v1/projects/{}", project.id);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(project.into()),
    ))
}

#[derive(Debug, Deserialize)]
struct ListProjectsQuery {
    cursor: Option<String>,
    #[serde(default = "default_project_limit")]
    limit: i64,
}

const fn default_project_limit() -> i64 {
    20
}

async fn list_projects(
    State(state): State<AppState>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Json<ProjectPageResponse>, ApiError> {
    let page = state
        .projects
        .list(LOCAL_OWNER_ID, query.cursor, query.limit)
        .await?;
    Ok(Json(ProjectPageResponse {
        items: page.items.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    }))
}

async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<ProjectResponse>, ApiError> {
    Ok(Json(
        state.projects.get(LOCAL_OWNER_ID, project_id).await?.into(),
    ))
}

async fn update_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project = state
        .projects
        .update(
            LOCAL_OWNER_ID,
            project_id,
            ProjectChanges {
                name: request.name,
                video_settings: request.video_settings.map(Into::into),
            },
        )
        .await?;
    Ok(Json(project.into()))
}

async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn readiness(State(state): State<AppState>) -> Result<Json<ReadinessResponse>, ApiError> {
    state.jobs.readiness().await.map_err(|_| {
        ApiError::service_unavailable("The database readiness check did not succeed.")
    })?;

    Ok(Json(ReadinessResponse {
        status: "ready",
        checks: vec![ReadinessCheck {
            name: "database",
            ready: true,
            detail: None,
        }],
    }))
}

async fn enqueue_probe(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let job = state.jobs.enqueue_probe("development-smoke-test").await?;
    Ok((StatusCode::CREATED, Json(job.into())))
}

async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<JobResponse>, ApiError> {
    Ok(Json(state.jobs.get(job_id).await?.into()))
}

async fn stream_job_events(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    state.jobs.get(job_id).await?;
    let mut cursor = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);

    let stream = async_stream::stream! {
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            match state.jobs.events_after(job_id, cursor).await {
                Ok(events) => {
                    let mut terminal = false;
                    for event in events {
                        cursor = event.id;
                        terminal = event.status.is_terminal();
                        let event_name = if terminal { event.status.as_str() } else { "progress" };
                        let response = JobEventResponse::from(event);
                        let sse = Event::default()
                            .id(cursor.to_string())
                            .event(event_name)
                            .json_data(response)
                            .expect("JobEventResponse serialization should succeed");
                        yield Ok(sse);
                    }
                    if terminal {
                        break;
                    }
                }
                Err(_) => {
                    let sse = Event::default()
                        .event("stream_error")
                        .data("Job event stream interrupted; poll the canonical job URL.");
                    yield Ok(sse);
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
