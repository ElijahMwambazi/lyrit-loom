import { createApiClient } from "@lyrit/api-client";
import type { components } from "@lyrit/api-client";
import {
  type FormEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

type Readiness = "checking" | "ready" | "unavailable";
type Project = components["schemas"]["Project"];

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
          <span className="brand-header-logo-frame" aria-hidden="true">
            <img
              src="/brand/lyrit-loom-logo.png"
              alt=""
              className="brand-header-logo"
            />
          </span>
          <span>Lyrit Loom</span>
        </div>
        <div className="header-readiness">
          <span className={`status-dot status-${readiness}`} aria-hidden="true" />
          {readiness === "ready" ? "System ready" : readiness}
        </div>
      </header>

      <section className="workspace-intro" aria-labelledby="page-title">
        <div>
          <div className="eyebrow">WEAVE WORDS INTO MOTION</div>
          <h1 id="page-title">Your projects</h1>
          <p className="lede">
            Start with a song, shape every word, and render motion that follows
            the music.
          </p>
        </div>
        <div className="hero-logo-frame">
          <img
            src="/brand/lyrit-loom-logo-mono.png"
            alt="Lyrit Loom woven waveform mark"
            className="hero-logo"
          />
        </div>
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
                      <span>Media setup next</span>
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

function formatProjectDate(timestamp: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(timestamp));
}
