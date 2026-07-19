import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

const project = {
  id: "00000000-0000-4000-8000-000000000010",
  name: "Midnight chorus",
  status: "draft",
  video_settings: {
    width: 1920,
    height: 1080,
    fps: 30,
    background_fit: "cover",
  },
  audio_asset: null,
  background_asset: null,
  active_transcript_revision: null,
  latest_render_id: null,
  created_at: "2026-07-17T10:00:00Z",
  updated_at: "2026-07-17T10:00:00Z",
};

function json(body: object, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function requestDetails(input: RequestInfo | URL, init?: RequestInit) {
  if (input instanceof Request) {
    return { url: input.url, method: input.method };
  }
  return { url: input.toString(), method: init?.method ?? "GET" };
}

describe("App", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const { url, method } = requestDetails(input, init);
        if (url.endsWith("/health/ready")) {
          return json({
            status: "ready",
            checks: [{ name: "database", ready: true }],
          });
        }
        if (url.includes("/projects") && method === "GET") {
          return json({ items: [], next_cursor: null });
        }
        throw new Error(`Unexpected request: ${method} ${url}`);
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows the branded project workspace and its empty state", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "Your projects" }),
    ).toBeInTheDocument();
    expect(screen.getByText("WEAVE WORDS INTO MOTION")).toBeInTheDocument();
    expect(screen.getByText("LL")).toHaveClass("brand-placeholder");
    expect(screen.getByText("Creative workspace")).toBeInTheDocument();
    expect(document.querySelector(".hero-logo")).not.toBeInTheDocument();
    expect(await screen.findByText("Your loom is ready.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New project" })).toBeEnabled();
  });

  it("creates a project from the workspace", async () => {
    vi.mocked(fetch).mockImplementation(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const { url, method } = requestDetails(input, init);
        if (url.endsWith("/health/ready")) {
          return json({ status: "ready", checks: [] });
        }
        if (url.includes("/projects") && method === "GET") {
          return json({ items: [], next_cursor: null });
        }
        if (url.endsWith("/projects") && method === "POST") {
          return json(project, 201);
        }
        throw new Error(`Unexpected request: ${method} ${url}`);
      },
    );

    render(<App />);
    const newProject = screen.getByRole("button", { name: "New project" });
    await waitFor(() => expect(newProject).toBeEnabled());
    fireEvent.click(newProject);
    fireEvent.change(screen.getByLabelText("Project name"), {
      target: { value: project.name },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    expect(
      await screen.findByRole("heading", { name: project.name }),
    ).toBeInTheDocument();
    expect(screen.getByText("1920 × 1080 · 30 fps")).toBeInTheDocument();
  });

  it("uploads source media with progress and refreshes the project", async () => {
    const audioAsset = {
      id: "00000000-0000-4000-8000-000000000020",
      project_id: project.id,
      kind: "audio",
      original_filename: "demo-song.mp3",
      media_type: "audio/mpeg",
      bytes: 2048,
      sha256: "a".repeat(64),
      duration_ms: 62_000,
      width: null,
      height: null,
      created_at: "2026-07-17T10:01:00Z",
    };
    vi.mocked(fetch).mockImplementation(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const { url, method } = requestDetails(input, init);
        if (url.endsWith("/health/ready")) {
          return json({ status: "ready", checks: [] });
        }
        if (url.endsWith(`/projects/${project.id}`) && method === "GET") {
          return json({ ...project, audio_asset: audioAsset });
        }
        if (url.includes("/projects") && method === "GET") {
          return json({ items: [project], next_cursor: null });
        }
        throw new Error(`Unexpected request: ${method} ${url}`);
      },
    );

    class XMLHttpRequestMock extends EventTarget {
      upload = new EventTarget();
      status = 0;
      responseText = "";
      method = "";
      url = "";

      open(method: string, url: string) {
        this.method = method;
        this.url = url;
      }

      setRequestHeader() {}

      send(body: FormData) {
        expect(this.method).toBe("POST");
        expect(this.url).toContain(`/projects/${project.id}/assets`);
        expect(body.get("kind")).toBe("audio");
        this.upload.dispatchEvent(
          new ProgressEvent("progress", {
            lengthComputable: true,
            loaded: 1,
            total: 2,
          }),
        );
        this.status = 201;
        queueMicrotask(() => this.dispatchEvent(new Event("load")));
      }
    }
    vi.stubGlobal("XMLHttpRequest", XMLHttpRequestMock);

    render(<App />);
    const input = await screen.findByLabelText("Choose or drop audio");
    fireEvent.change(input, {
      target: {
        files: [new File(["fake audio"], "demo-song.mp3", { type: "audio/mpeg" })],
      },
    });

    expect(await screen.findByText("demo-song.mp3")).toBeInTheDocument();
    expect(screen.getByText("1:02 · 2 KB")).toBeInTheDocument();
  });

  it("receives worker progress and displays the completed result", async () => {
    vi.mocked(fetch).mockImplementation(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const { url, method } = requestDetails(input, init);
        if (url.endsWith("/health/ready")) {
          return json({ status: "ready", checks: [] });
        }
        if (url.includes("/projects") && method === "GET") {
          return json({ items: [], next_cursor: null });
        }
        if (url.endsWith("/internal/dev/jobs/probe") && method === "POST") {
          return json(
            {
              id: "00000000-0000-4000-8000-000000000001",
              status: "queued",
              phase: "queued",
              progress: 0,
            },
            201,
          );
        }
        if (url.endsWith("/jobs/00000000-0000-4000-8000-000000000001")) {
          return json({
            id: "00000000-0000-4000-8000-000000000001",
            status: "succeeded",
            phase: "complete",
            progress: 1,
            result: { message: "Durable job queue is operational" },
          });
        }
        throw new Error(`Unexpected request: ${method} ${url}`);
      },
    );

    class EventSourceMock {
      static instance: EventSourceMock;
      listeners = new Map<string, EventListener>();
      onerror: ((event: Event) => void) | null = null;

      constructor(public readonly url: string) {
        EventSourceMock.instance = this;
      }

      addEventListener(type: string, listener: EventListener) {
        this.listeners.set(type, listener);
      }

      close() {}

      emit(type: string, data: object) {
        this.listeners.get(type)?.(
          new MessageEvent(type, { data: JSON.stringify(data) }),
        );
      }
    }

    vi.stubGlobal("EventSource", EventSourceMock);
    render(<App />);

    const button = await screen.findByRole("button", { name: "Run probe" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    await waitFor(() => expect(EventSourceMock.instance).toBeDefined());

    EventSourceMock.instance.emit("progress", {
      status: "running",
      phase: "checking_infrastructure",
      progress: 0.55,
    });
    expect(await screen.findByText("55%")).toBeInTheDocument();

    EventSourceMock.instance.emit("succeeded", {
      status: "succeeded",
      phase: "complete",
      progress: 1,
    });
    expect(
      await screen.findByText("Durable job queue is operational"),
    ).toBeInTheDocument();
  });

  it("queues transcription and displays the completed word transcript", async () => {
    const audioAsset = {
      id: "00000000-0000-4000-8000-000000000020",
      project_id: project.id,
      kind: "audio",
      original_filename: "demo-song.mp3",
      media_type: "audio/mpeg",
      bytes: 2048,
      sha256: "a".repeat(64),
      duration_ms: 62_000,
      width: null,
      height: null,
      created_at: "2026-07-17T10:01:00Z",
    };
    const projectWithAudio = { ...project, audio_asset: audioAsset };
    const transcript = {
      id: "00000000-0000-4000-8000-000000000040",
      project_id: project.id,
      audio_asset_id: audioAsset.id,
      revision: 1,
      source: "whisper",
      language: "en",
      duration_ms: 4200,
      cues: [
        {
          id: "00000000-0000-4000-8000-000000000041",
          start_ms: 0,
          end_ms: 1200,
          words: [
            {
              id: "00000000-0000-4000-8000-000000000042",
              text: "Weave",
              start_ms: 0,
              end_ms: 500,
              confidence: 0.99,
            },
            {
              id: "00000000-0000-4000-8000-000000000043",
              text: "motion",
              start_ms: 600,
              end_ms: 1200,
              confidence: 0.72,
            },
          ],
        },
      ],
      transcriber: {
        engine: "fake",
        model: "configured-default",
        revision: "milestone-2-fake",
        language_probability: 1,
      },
      created_at: "2026-07-19T10:00:00Z",
    };
    let transcriptReady = false;
    vi.mocked(fetch).mockImplementation(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const { url, method } = requestDetails(input, init);
        if (url.endsWith("/health/ready")) {
          return json({ status: "ready", checks: [] });
        }
        if (url.endsWith(`/projects/${project.id}/transcriptions`) && method === "POST") {
          return json({
            job: {
              id: "00000000-0000-4000-8000-000000000030",
              status: "queued",
              phase: "queued",
              progress: 0,
            },
            job_url: `/api/v1/jobs/00000000-0000-4000-8000-000000000030`,
            events_url: `/api/v1/jobs/00000000-0000-4000-8000-000000000030/events`,
          }, 202);
        }
        if (url.endsWith(`/projects/${project.id}/transcript`) && method === "GET") {
          return transcriptReady ? json(transcript) : json({}, 404);
        }
        if (url.endsWith(`/projects/${project.id}`) && method === "GET") {
          return json({ ...projectWithAudio, active_transcript_revision: 1 });
        }
        if (url.endsWith("/projects?limit=20") && method === "GET") {
          return json({ items: [projectWithAudio], next_cursor: null });
        }
        throw new Error(`Unexpected request: ${method} ${url}`);
      },
    );

    class EventSourceMock {
      static instance: EventSourceMock;
      listeners = new Map<string, EventListener>();
      onerror: ((event: Event) => void) | null = null;

      constructor(public readonly url: string) {
        EventSourceMock.instance = this;
      }

      addEventListener(type: string, listener: EventListener) {
        this.listeners.set(type, listener);
      }

      close() {}

      emit(type: string, data: object) {
        this.listeners.get(type)?.(
          new MessageEvent(type, { data: JSON.stringify(data) }),
        );
      }
    }
    vi.stubGlobal("EventSource", EventSourceMock);

    render(<App />);
    const button = await screen.findByRole("button", { name: "Transcribe audio" });
    fireEvent.click(button);
    await waitFor(() => expect(EventSourceMock.instance).toBeDefined());
    EventSourceMock.instance.emit("progress", {
      status: "running",
      phase: "transcribing",
      progress: 0.65,
    });
    expect(await screen.findByText("transcribing · 65%")).toBeInTheDocument();

    transcriptReady = true;
    EventSourceMock.instance.emit("succeeded", {
      status: "succeeded",
      phase: "complete",
      progress: 1,
    });
    expect(await screen.findByRole("button", { name: /Weave, 99% confidence/ })).toBeInTheDocument();
    expect(screen.getByLabelText(`${project.name} source audio`)).toHaveAttribute(
      "src",
      `/api/v1/artifacts/${audioAsset.id}/content`,
    );
    expect(
      screen.getByRole("button", {
        name: "motion, 72% confidence, review suggested, starts at 0:00",
      }),
    ).toHaveClass("confidence-review");
  });
});
