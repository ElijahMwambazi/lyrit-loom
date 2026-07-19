use lyrit_application::{AssetService, JobService, ProjectService, TranscriptService};
use lyrit_media::{FfprobeMediaInspector, LocalArtifactStore};
use lyrit_persistence::{
    PgAssetRepository, PgJobRepository, PgProjectRepository, PgTranscriptRepository,
};

pub type AppAssetService =
    AssetService<PgAssetRepository, PgProjectRepository, LocalArtifactStore, FfprobeMediaInspector>;

#[derive(Clone)]
pub struct AppState {
    pub jobs: JobService<PgJobRepository>,
    pub projects: ProjectService<PgProjectRepository>,
    pub assets: AppAssetService,
    pub transcripts: TranscriptService<PgTranscriptRepository>,
    pub max_upload_bytes: usize,
    pub enable_dev_routes: bool,
}
