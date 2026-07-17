import { createApiClient } from "@lyrit/api-client";
import { useCallback, useEffect, useRef, useState } from "react";

type Readiness = "checking" | "ready" | "unavailable";

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

export function App() {
  const [readiness, setReadiness] = useState<Readiness>("checking");
  const [probe, setProbe] = useState<ProbeJob | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);

  const checkReadiness = useCallback(async () => {
    setReadiness("checking");
    const api = createApiClient(apiBaseUrl);
    const { data, error } = await api.GET("/health/ready");
    setReadiness(!error && data?.status === "ready" ? "ready" : "unavailable");
  }, []);

  useEffect(() => {
    void checkReadiness();
    return () => eventSourceRef.current?.close();
  }, [checkReadiness]);

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
      const event = JSON.parse((message as MessageEvent<string>).data) as {
        status: string;
        phase: string;
        progress: number;
      };
      setProbe((current) => current && { ...current, ...event });
    });

    events.addEventListener("succeeded", async () => {
      events.close();
      const latest = await fetch(`${apiBaseUrl}/jobs/${initial.id}`);
      if (latest.ok) {
        setProbe((await latest.json()) as ProbeJob);
      }
    });

    events.addEventListener("failed", () => {
      events.close();
      setProbeError("The worker reported a failed probe.");
    });

    events.onerror = () => {
      events.close();
    };
  }

  const progress = Math.round((probe?.progress ?? 0) * 100);

  return (
    <main className="app-shell">
      <header className="brand-header" aria-label="Lyrit Loom">
        <img
          src="/brand/lyrit-loom-logo-mono.png"
          alt=""
          className="brand-header-logo"
        />
        <span>Lyrit Loom</span>
      </header>
      <section className="hero" aria-labelledby="page-title">
        <div className="hero-copy">
          <div className="eyebrow">PRIVATE CREATIVE TOOLING</div>
          <h1 id="page-title">Lyrit Loom</h1>
          <p className="tagline">Weave words into motion.</p>
          <p className="lede">
            Rust orchestration, editable word timing, deterministic ASS, and an
            FFmpeg render pipeline.
          </p>
        </div>
        <img
          src="/brand/lyrit-loom-logo.png"
          alt="Lyrit Loom woven waveform mark"
          className="hero-logo"
        />

        <div className="system-card">
          <div>
            <span
              className={`status-dot status-${readiness}`}
              aria-hidden="true"
            />
            <strong>API and database</strong>
            <p>
              {readiness === "checking" && "Checking the local system…"}
              {readiness === "ready" && "Ready for the first project workflow."}
              {readiness === "unavailable" &&
                "Unavailable—start the API and PostgreSQL."}
            </p>
          </div>
          <button
            type="button"
            className="button-secondary"
            onClick={() => void checkReadiness()}
          >
            Recheck
          </button>
        </div>

        <div className="probe-card">
          <div className="probe-heading">
            <div>
              <span className="step-label">MILESTONE 0 PROBE</span>
              <h2>Verify the durable worker</h2>
            </div>
            <button
              type="button"
              className="button-primary"
              onClick={() => void runProbe()}
              disabled={
                readiness !== "ready" ||
                probe?.status === "queued" ||
                probe?.status === "running"
              }
            >
              Run probe job
            </button>
          </div>

          <div className="progress-track" aria-label="Probe progress">
            <div className="progress-fill" style={{ width: `${progress}%` }} />
          </div>
          <div className="probe-meta">
            <span>
              {probe ? probe.phase.replaceAll("_", " ") : "not started"}
            </span>
            <span>{progress}%</span>
          </div>
          {probe?.status === "succeeded" && (
            <p className="success-message">
              {probe.result?.message ?? "Durable job queue is operational."}
            </p>
          )}
          {probeError && <p className="error-message">{probeError}</p>}
        </div>
      </section>

      <section className="pipeline" aria-label="Planned product pipeline">
        {[
          ["01", "Upload", "Audio and background media"],
          ["02", "Transcribe", "Whisper word timestamps"],
          ["03", "Edit", "Waveform-aligned lyrics"],
          ["04", "Render", "ASS and FFmpeg output"],
        ].map(([number, title, detail]) => (
          <article key={number}>
            <span>{number}</span>
            <h3>{title}</h3>
            <p>{detail}</p>
          </article>
        ))}
      </section>
    </main>
  );
}
