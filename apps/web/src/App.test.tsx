import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";

describe("App", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          new Response(
            JSON.stringify({
              status: "ready",
              checks: [{ name: "database", ready: true }],
            }),
            { status: 200, headers: { "Content-Type": "application/json" } },
          ),
        ),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("introduces the product and its first executable probe", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "Lyrit Loom" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Run probe job" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Weave words into motion.")).toBeInTheDocument();
    expect(
      screen.getByAltText("Lyrit Loom woven waveform mark"),
    ).toHaveAttribute("src", "/brand/lyrit-loom-logo-mono.png");
    expect(document.querySelector(".brand-header-logo")).toHaveAttribute(
      "src",
      "/brand/lyrit-loom-logo.png",
    );
    expect(
      await screen.findByText("Ready for the first project workflow."),
    ).toBeInTheDocument();
  });

  it("receives worker progress and displays the completed result", async () => {
    const fetchMock = vi.mocked(fetch);
    fetchMock
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            status: "ready",
            checks: [{ name: "database", ready: true }],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            id: "00000000-0000-4000-8000-000000000001",
            status: "queued",
            phase: "queued",
            progress: 0,
          }),
          { status: 201, headers: { "Content-Type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            id: "00000000-0000-4000-8000-000000000001",
            status: "succeeded",
            phase: "complete",
            progress: 1,
            result: { message: "Durable job queue is operational" },
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
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

    const button = await screen.findByRole("button", { name: "Run probe job" });
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
