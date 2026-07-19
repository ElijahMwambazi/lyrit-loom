import { createApiClient } from "@lyrit/api-client";
import type { components } from "@lyrit/api-client";
import {
  type DragEvent,
  type FormEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

type Readiness = "checking" | "ready" | "unavailable";
type Project = components["schemas"]["Project"];
type Asset = components["schemas"]["Asset"];
type SourceAssetKind = components["schemas"]["SourceAssetKind"];

type UploadState = {
  progress: number;
  status: "uploading" | "failed";
  error?: string;
};

type ProbeJob = {
  id: string;
  status: string;
  phase: string;
  progress: number;
  result?: { message?: string } | null;
};

const apiBaseUrl = new URL(
  import.meta.env.VITE_API_BASE_URL ?? "/api/v1",
  window.location.origin,
)
  .toString()
  .replace(/\/$/, "");
const api = createApiClient(apiBaseUrl);

export function App() {
  const [readiness, setReadiness] = useState<Readiness>("checking");
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(true);
  const [projectsError, setProjectsError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [creating, setCreating] = useState(false);
  const [editingProjectId, setEditingProjectId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [uploads, setUploads] = useState<Record<string, UploadState>>({});
  const [probe, setProbe] = useState<ProbeJob | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);

  const checkReadiness = useCallback(async () => {
    setReadiness("checking");
    const { data, error } = await api.GET("/health/ready");
    setReadiness(!error && data?.status === "ready" ? "ready" : "unavailable");
  }, []);

  const loadProjects = useCallback(async () => {
    setProjectsLoading(true);
    setProjectsError(null);
    const { data, error } = await api.GET("/projects", {
      params: { query: { limit: 20 } },
    });
    if (error || !data) {
      setProjectsError("Projects could not be loaded. Check the local API.");
    } else {
      setProjects(data.items);
    }
    setProjectsLoading(false);
  }, []);

  useEffect(() => {
    void checkReadiness();
    void loadProjects();
    return () => eventSourceRef.current?.close();
  }, [checkReadiness, loadProjects]);

  async function createProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = newProjectName.trim();
    if (!name) return;

    setCreating(true);
    setProjectsError(null);
    const { data, error } = await api.POST("/projects", { body: { name } });
    if (error || !data) {
      setProjectsError("The project could not be created.");
    } else {
      setProjects((current) => [data, ...current]);
      setNewProjectName("");
      setShowCreate(false);
    }
    setCreating(false);
  }

  async function renameProject(event: FormEvent<HTMLFormElement>, project: Project) {
    event.preventDefault();
    const name = editingName.trim();
    if (!name || name === project.name) {
      setEditingProjectId(null);
      return;
    }

    const { data, error } = await api.PATCH("/projects/{project_id}", {
      params: { path: { project_id: project.id } },
      body: { name },
    });
    if (error || !data) {
      setProjectsError("The project name could not be updated.");
      return;
    }
    setProjects((current) =>
      current.map((candidate) => (candidate.id === data.id ? data : candidate)),
    );
    setEditingProjectId(null);
  }

  function uploadAsset(project: Project, kind: SourceAssetKind, file: File) {
    const key = uploadKey(project.id, kind);
    setUploads((current) => ({
      ...current,
      [key]: { progress: 0, status: "uploading" },
    }));
    const form = new FormData();
    form.append("kind", kind);
    form.append("file", file);
    const request = new XMLHttpRequest();
    request.open("POST", `${apiBaseUrl}/projects/${project.id}/assets`);
    request.setRequestHeader("Accept", "application/json");
    request.upload.addEventListener("progress", (event) => {
      if (!event.lengthComputable) return;
      setUploads((current) => ({
        ...current,
        [key]: {
          progress: Math.round((event.loaded / event.total) * 100),
          status: "uploading",
        },
      }));
    });
    request.addEventListener("load", () => {
      if (request.status < 200 || request.status >= 300) {
        setUploadFailure(key, uploadError(request));
        return;
      }
      void refreshProject(project.id, key);
    });
    request.addEventListener("error", () => {
      setUploadFailure(key, "The upload was interrupted. Please try again.");
    });
    request.send(form);
  }

  async function refreshProject(projectId: string, uploadStateKey: string) {
    const { data, error } = await api.GET("/projects/{project_id}", {
      params: { path: { project_id: projectId } },
    });
    if (error || !data) {
      setUploadFailure(
        uploadStateKey,
        "The asset was stored, but the project could not be refreshed.",
      );
      return;
    }
    setProjects((current) =>
      current.map((project) => (project.id === data.id ? data : project)),
    );
    setUploads((current) => {
      const next = { ...current };
      delete next[uploadStateKey];
      return next;
    });
  }

  function setUploadFailure(key: string, error: string) {
    setUploads((current) => ({
      ...current,
      [key]: { progress: 0, status: "failed", error },
    }));
  }

  function handleDrop(
    event: DragEvent<HTMLLabelElement>,
    project: Project,
    kind: SourceAssetKind,
  ) {
    event.preventDefault();
    const file = event.dataTransfer.files[0];
    if (file) uploadAsset(project, kind, file);
  }

  async function runProbe() {
    eventSourceRef.current?.close();
    setProbeError(null);
    setProbe(null);

    const response = await fetch(`${apiBaseUrl}/internal/dev/jobs/probe`, {
      method: "POST",
      headers: { Accept: "application/json" },
    });
    if (!response.ok) {
      setProbeError("The development probe could not be queued.");
      return;
    }

    const initial = (await response.json()) as ProbeJob;
    setProbe(initial);
    const events = new EventSource(`${apiBaseUrl}/jobs/${initial.id}/events`);
    eventSourceRef.current = events;
    events.addEventListener("progress", (message) => {
      const update = JSON.parse((message as MessageEvent<string>).data) as Pick<
        ProbeJob,
        "status" | "phase" | "progress"
      >;
      setProbe((current) => current && { ...current, ...update });
    });
    events.addEventListener("succeeded", async () => {
      events.close();
      const latest = await fetch(`${apiBaseUrl}/jobs/${initial.id}`);
      if (latest.ok) setProbe((await latest.json()) as ProbeJob);
    });
    events.addEventListener("failed", () => {
      events.close();
      setProbeError("The worker reported a failed probe.");
    });
    events.onerror = () => events.close();
  }

  const progress = Math.round((probe?.progress ?? 0) * 100);

  return (
    <main className="app-shell">
      <header className="brand-header" aria-label="Lyrit Loom">
        <div className="brand-lockup">
          <span className="brand-placeholder" aria-hidden="true">LL</span>
          <span className="brand-name">Lyrit Loom</span>
          <span className="brand-context">Creative workspace</span>
        </div>
        <div className="header-readiness">
          <span className={`status-dot status-${readiness}`} aria-hidden="true" />
          {readiness === "ready" ? "System ready" : readiness}
        </div>
      </header>

      <section className="workspace-intro" aria-labelledby="page-title">
        <div className="eyebrow">WEAVE WORDS INTO MOTION</div>
        <h1 id="page-title">Your projects</h1>
        <p className="lede">
          Start with a song, shape every word, and render motion that follows
          the music.
        </p>
      </section>

      <section className="projects-section" aria-labelledby="projects-title">
        <div className="section-heading">
          <div>
            <span className="step-label">PROJECT LIBRARY</span>
            <h2 id="projects-title">Continue creating</h2>
          </div>
          <button
            type="button"
            className="button-primary"
            onClick={() => setShowCreate((visible) => !visible)}
            disabled={readiness !== "ready"}
          >
            {showCreate ? "Cancel" : "New project"}
          </button>
        </div>

        {showCreate && (
          <form className="create-project-card" onSubmit={(event) => void createProject(event)}>
            <label htmlFor="project-name">Project name</label>
            <div className="create-project-row">
              <input
                id="project-name"
                value={newProjectName}
                maxLength={120}
                autoFocus
                placeholder="Midnight chorus"
                onChange={(event) => setNewProjectName(event.target.value)}
              />
              <button className="button-primary" type="submit" disabled={creating}>
                {creating ? "Creating…" : "Create project"}
              </button>
            </div>
            <p>1920 × 1080, 30 fps. Video settings can be refined later.</p>
          </form>
        )}

        {projectsError && <p className="error-message">{projectsError}</p>}
        {projectsLoading ? (
          <div className="projects-empty">Loading your projects…</div>
        ) : projects.length === 0 ? (
          <div className="projects-empty">
            <strong>Your loom is ready.</strong>
            <p>Create the first project, then add audio and background media.</p>
          </div>
        ) : (
          <div className="projects-grid">
            {projects.map((project) => (
              <article className="project-card" key={project.id}>
                <div className="project-card-topline">
                  <span className={`project-status status-${project.status}`}>
                    {project.status}
                  </span>
                  <span>{formatProjectDate(project.updated_at)}</span>
                </div>
                {editingProjectId === project.id ? (
                  <form onSubmit={(event) => void renameProject(event, project)}>
                    <input
                      aria-label={`Rename ${project.name}`}
                      value={editingName}
                      maxLength={120}
                      autoFocus
                      onChange={(event) => setEditingName(event.target.value)}
                    />
                    <div className="project-card-actions">
                      <button className="text-button" type="submit">Save</button>
                      <button
                        className="text-button"
                        type="button"
                        onClick={() => setEditingProjectId(null)}
                      >
                        Cancel
                      </button>
                    </div>
                  </form>
                ) : (
                  <>
                    <h3>{project.name}</h3>
                    <p>
                      {project.video_settings.width} × {project.video_settings.height} ·{" "}
                      {project.video_settings.fps} fps
                    </p>
                    <div className="project-card-actions">
                      <button
                        className="text-button"
                        type="button"
                        onClick={() => {
                          setEditingProjectId(project.id);
                          setEditingName(project.name);
                        }}
                      >
                        Rename
                      </button>
                      <span>
                        {project.audio_asset && project.background_asset
                          ? "Sources ready"
                          : "Add source media"}
                      </span>
                    </div>
                    <div className="asset-slots">
                      <AssetUpload
                        project={project}
                        kind="audio"
                        asset={project.audio_asset}
                        state={uploads[uploadKey(project.id, "audio")]}
                        onFile={(file) => uploadAsset(project, "audio", file)}
                        onDrop={(event) => handleDrop(event, project, "audio")}
                      />
                      <AssetUpload
                        project={project}
                        kind="background"
                        asset={project.background_asset}
                        state={uploads[uploadKey(project.id, "background")]}
                        onFile={(file) => uploadAsset(project, "background", file)}
                        onDrop={(event) =>
                          handleDrop(event, project, "background")
                        }
                      />
                    </div>
                  </>
                )}
              </article>
            ))}
          </div>
        )}
      </section>

      <details className="diagnostics">
        <summary>Foundation diagnostics</summary>
        <div className="diagnostics-content">
          <div>
            <span className="step-label">DURABLE WORKER</span>
            <h2>Run the Milestone 0 probe</h2>
          </div>
          <button
            type="button"
            className="button-secondary"
            onClick={() => void runProbe()}
            disabled={readiness !== "ready" || probe?.status === "running"}
          >
            Run probe
          </button>
          <div className="progress-track" aria-label="Probe progress">
            <div className="progress-fill" style={{ width: `${progress}%` }} />
          </div>
          <div className="probe-meta">
            <span>{probe ? probe.phase.replaceAll("_", " ") : "not started"}</span>
            <span>{progress}%</span>
          </div>
          {probe?.status === "succeeded" && (
            <p className="success-message">
              {probe.result?.message ?? "Durable job queue is operational."}
            </p>
          )}
          {probeError && <p className="error-message">{probeError}</p>}
        </div>
      </details>
    </main>
  );
}

type AssetUploadProps = {
  project: Project;
  kind: SourceAssetKind;
  asset: Asset | null | undefined;
  state: UploadState | undefined;
  onFile: (file: File) => void;
  onDrop: (event: DragEvent<HTMLLabelElement>) => void;
};

function AssetUpload({
  project,
  kind,
  asset,
  state,
  onFile,
  onDrop,
}: AssetUploadProps) {
  const label = kind === "audio" ? "Audio" : "Background";
  const inputId = `${project.id}-${kind}-upload`;
  const uploading = state?.status === "uploading";
  return (
    <div className={`asset-slot ${asset ? "asset-present" : ""}`}>
      <div className="asset-slot-heading">
        <strong>{label}</strong>
        {asset && <span>Ready</span>}
      </div>
      {asset ? (
        <div className="asset-summary">
          <span title={asset.original_filename ?? undefined}>
            {asset.original_filename ?? "Source media"}
          </span>
          <small>{assetDescription(asset)}</small>
        </div>
      ) : (
        <p>{kind === "audio" ? "MP3, WAV, FLAC, OGG" : "PNG, JPEG, or WebP"}</p>
      )}
      <label
        className="asset-dropzone"
        htmlFor={inputId}
        onDragOver={(event) => event.preventDefault()}
        onDrop={onDrop}
      >
        {uploading
          ? `Uploading ${state.progress}%`
          : asset
            ? `Replace ${label.toLowerCase()}`
            : `Choose or drop ${label.toLowerCase()}`}
      </label>
      <input
        id={inputId}
        className="visually-hidden"
        type="file"
        accept={kind === "audio" ? "audio/*" : "image/png,image/jpeg,image/webp"}
        disabled={uploading}
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) onFile(file);
          event.target.value = "";
        }}
      />
      {uploading && (
        <div className="upload-progress" aria-label={`${label} upload progress`}>
          <span style={{ width: `${state.progress}%` }} />
        </div>
      )}
      {state?.status === "failed" && (
        <small className="upload-error">{state.error}</small>
      )}
    </div>
  );
}

function formatProjectDate(timestamp: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(timestamp));
}

function uploadKey(projectId: string, kind: SourceAssetKind) {
  return `${projectId}:${kind}`;
}

function assetDescription(asset: Asset) {
  if (asset.kind === "audio" && asset.duration_ms != null) {
    const totalSeconds = Math.round(asset.duration_ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = String(totalSeconds % 60).padStart(2, "0");
    return `${minutes}:${seconds} · ${formatBytes(asset.bytes)}`;
  }
  if (asset.width && asset.height) {
    return `${asset.width} × ${asset.height} · ${formatBytes(asset.bytes)}`;
  }
  return formatBytes(asset.bytes);
}

function formatBytes(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function uploadError(request: XMLHttpRequest) {
  try {
    const problem = JSON.parse(request.responseText) as { detail?: string };
    if (problem.detail) return problem.detail;
  } catch {
    // Fall through to a stable client message.
  }
  if (request.status === 413) return "This file exceeds the upload size limit.";
  if (request.status === 415) return "This media format is not supported.";
  return "The media could not be uploaded.";
}
