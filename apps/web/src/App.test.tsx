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
    expect(
      screen.getByAltText("Lyrit Loom woven waveform mark"),
    ).toHaveAttribute("src", "/brand/lyrit-loom-logo-mono.png");
    expect(document.querySelector(".brand-header-logo")).toHaveAttribute(
      "src",
      "/brand/lyrit-loom-logo.png",
    );
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
});
