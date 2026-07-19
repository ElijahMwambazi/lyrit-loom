import "@testing-library/jest-dom/vitest";
import { beforeEach, vi } from "vitest";

type WaveSurferEvent =
  | "loading"
  | "ready"
  | "timeupdate"
  | "interaction"
  | "play"
  | "pause"
  | "finish"
  | "error";

type Listener = (...values: never[]) => void;

export class WaveSurferTestDouble {
  readonly listeners = new Map<WaveSurferEvent, Set<Listener>>();
  currentTime = 0;
  duration = 4.2;
  playing = false;
  playbackRate = 1;
  destroyed = false;

  on(event: WaveSurferEvent, listener: Listener) {
    const listeners = this.listeners.get(event) ?? new Set<Listener>();
    listeners.add(listener);
    this.listeners.set(event, listeners);
    if (event === "ready") {
      queueMicrotask(() => this.emit("ready", this.duration));
    }
    return () => listeners.delete(listener);
  }

  emit(event: WaveSurferEvent, ...values: unknown[]) {
    this.listeners.get(event)?.forEach((listener) => listener(...(values as never[])));
  }

  setTime(seconds: number) {
    this.currentTime = seconds;
    this.emit("timeupdate", seconds);
  }

  getCurrentTime() {
    return this.currentTime;
  }

  isPlaying() {
    return this.playing;
  }

  async play() {
    this.playing = true;
    this.emit("play");
  }

  async playPause() {
    this.playing = !this.playing;
    this.emit(this.playing ? "play" : "pause");
  }

  setPlaybackRate(rate: number) {
    this.playbackRate = rate;
  }

  destroy() {
    this.destroyed = true;
  }
}

export const waveSurferTestDoubles: WaveSurferTestDouble[] = [];

vi.mock("wavesurfer.js", () => ({
  default: {
    create: () => {
      const waveform = new WaveSurferTestDouble();
      waveSurferTestDoubles.push(waveform);
      return waveform;
    },
  },
}));

beforeEach(() => {
  waveSurferTestDoubles.length = 0;
});
