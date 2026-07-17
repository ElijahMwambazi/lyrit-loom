use lyrit_application::JobService;
use lyrit_persistence::PgJobRepository;

#[derive(Clone)]
pub struct AppState {
    pub jobs: JobService<PgJobRepository>,
    pub enable_dev_routes: bool,
}
