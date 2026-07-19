import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";

import { waveSurferTestDoubles } from "./test/setup";
import {
  WaveformTimeline,
  type WaveformTimelineHandle,
} from "./WaveformTimeline";

const words = [
  { id: "word-1", text: "Weave", start_ms: 0, end_ms: 500 },
  { id: "word-2", text: "motion", start_ms: 600, end_ms: 1200 },
];

describe("WaveformTimeline", () => {
  it("synchronizes transport position, word selection, and playback controls", async () => {
    const onPositionChange = vi.fn();
    const onSelectWord = vi.fn();
    const ref = createRef<WaveformTimelineHandle>();

    const { rerender } = render(
      <WaveformTimeline
        ref={ref}
        label="Midnight chorus"
        src="/api/v1/artifacts/audio/content"
        durationMs={4200}
        words={words}
        activeWordId={null}
        onPositionChange={onPositionChange}
        onSelectWord={onSelectWord}
      />,
    );

    expect(await screen.findByRole("button", { name: "Play audio" })).toBeEnabled();
    expect(waveSurferTestDoubles).toHaveLength(1);
    const waveform = waveSurferTestDoubles[0]!;

    fireEvent.click(screen.getByRole("button", { name: "Play audio" }));
    expect(await screen.findByRole("button", { name: "Pause audio" })).toBeEnabled();

    act(() => waveform.emit("timeupdate", 0.7));
    expect(onPositionChange).toHaveBeenLastCalledWith(700);
    expect(screen.getByText("0:00.7 / 0:04.2")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "motion, starts at 0:00.6" }));
    expect(onSelectWord).toHaveBeenCalledWith("word-2");
    expect(waveform.currentTime).toBe(0.6);

    rerender(
      <WaveformTimeline
        ref={ref}
        label="Midnight chorus"
        src="/api/v1/artifacts/audio/content"
        durationMs={4200}
        words={words}
        activeWordId="word-2"
        onPositionChange={onPositionChange}
        onSelectWord={onSelectWord}
      />,
    );
    expect(
      screen.getByRole("button", { name: "motion, starts at 0:00.6" }),
    ).toHaveAttribute("aria-pressed", "true");

    fireEvent.change(screen.getByLabelText("Playback speed"), {
      target: { value: "1.25" },
    });
    expect(waveform.playbackRate).toBe(1.25);

    act(() => ref.current?.seekToMs(1100));
    expect(waveform.currentTime).toBe(1.1);
    await waitFor(() => expect(onPositionChange).toHaveBeenLastCalledWith(1100));
  });

  it("provides keyboard play and seek controls when the timeline is focused", async () => {
    render(
      <WaveformTimeline
        label="Midnight chorus"
        src="/audio.mp3"
        durationMs={20_000}
        words={words}
        activeWordId={null}
        onPositionChange={() => undefined}
        onSelectWord={() => undefined}
      />,
    );
    const timeline = screen.getByRole("region", {
      name: "Midnight chorus waveform and transport",
    });
    await waitFor(() => expect(screen.getByRole("button", { name: "Play audio" })).toBeEnabled());
    const waveform = waveSurferTestDoubles[0]!;

    timeline.focus();
    fireEvent.keyDown(timeline, { key: " " });
    expect(waveform.playing).toBe(true);
    waveform.currentTime = 10;
    fireEvent.keyDown(timeline, { key: "ArrowLeft" });
    expect(waveform.currentTime).toBe(5);
    fireEvent.keyDown(timeline, { key: "ArrowRight" });
    expect(waveform.currentTime).toBe(10);
  });
});
