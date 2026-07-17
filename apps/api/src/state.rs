use lyrit_application::{JobService, ProjectService};
use lyrit_persistence::{PgJobRepository, PgProjectRepository};

#[derive(Clone)]
pub struct AppState {
    pub jobs: JobService<PgJobRepository>,
    pub projects: ProjectService<PgProjectRepository>,
    pub enable_dev_routes: bool,
}
