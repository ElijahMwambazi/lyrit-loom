import {
  forwardRef,
  type KeyboardEvent,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import WaveSurfer from "wavesurfer.js";

export type TimelineWord = {
  id: string;
  text: string;
  start_ms: number;
  end_ms: number;
};

export type WaveformTimelineHandle = {
  seekToMs: (positionMs: number, autoplay?: boolean) => void;
};

type WaveformTimelineProps = {
  label: string;
  src: string;
  durationMs: number;
  words: TimelineWord[];
  activeWordId: string | null;
  onPositionChange: (positionMs: number) => void;
  onSelectWord: (wordId: string) => void;
};

export const WaveformTimeline = forwardRef<
  WaveformTimelineHandle,
  WaveformTimelineProps
>(function WaveformTimeline(
  {
    label,
    src,
    durationMs,
    words,
    activeWordId,
    onPositionChange,
    onSelectWord,
  },
  ref,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const waveSurferRef = useRef<WaveSurfer | null>(null);
  const onPositionChangeRef = useRef(onPositionChange);
  const [ready, setReady] = useState(false);
  const [loading, setLoading] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [positionMs, setPositionMs] = useState(0);
  const [decodedDurationMs, setDecodedDurationMs] = useState(durationMs);
  const [playbackRate, setPlaybackRate] = useState(1);
  const [waveformError, setWaveformError] = useState(false);

  useEffect(() => {
    onPositionChangeRef.current = onPositionChange;
  }, [onPositionChange]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    setReady(false);
    setLoading(0);
    setPlaying(false);
    setPositionMs(0);
    setWaveformError(false);

    let waveform: WaveSurfer;
    try {
      waveform = WaveSurfer.create({
        container,
        url: src,
        height: 72,
        waveColor: "#434955",
        progressColor: "#4da3ff",
        cursorColor: "#f0c65a",
        cursorWidth: 2,
        barWidth: 2,
        barGap: 2,
        barRadius: 2,
        barMinHeight: 2,
        dragToSeek: true,
        normalize: true,
      });
    } catch {
      setWaveformError(true);
      return;
    }
    waveSurferRef.current = waveform;

    const updatePosition = (seconds: number) => {
      const nextPositionMs = Math.max(0, Math.round(seconds * 1000));
      setPositionMs(nextPositionMs);
      onPositionChangeRef.current(nextPositionMs);
    };
    const unsubscribe = [
      waveform.on("loading", setLoading),
      waveform.on("ready", (seconds) => {
        setDecodedDurationMs(Math.round(seconds * 1000));
        setReady(true);
        setLoading(100);
      }),
      waveform.on("timeupdate", updatePosition),
      waveform.on("interaction", updatePosition),
      waveform.on("play", () => setPlaying(true)),
      waveform.on("pause", () => setPlaying(false)),
      waveform.on("finish", () => setPlaying(false)),
      waveform.on("error", () => {
        setWaveformError(true);
        setPlaying(false);
      }),
    ];

    return () => {
      unsubscribe.forEach((removeListener) => removeListener());
      waveSurferRef.current = null;
      waveform.destroy();
    };
  }, [src]);

  function seekToMs(nextPositionMs: number, autoplay = false) {
    const waveform = waveSurferRef.current;
    if (!waveform) return;
    const duration = Math.max(decodedDurationMs, durationMs, 1);
    const clampedPositionMs = Math.min(Math.max(0, nextPositionMs), duration);
    waveform.setTime(clampedPositionMs / 1000);
    if (autoplay && !waveform.isPlaying()) {
      void waveform.play().catch(() => undefined);
    }
  }

  useImperativeHandle(ref, () => ({ seekToMs }));

  function togglePlayback() {
    void waveSurferRef.current?.playPause().catch(() => undefined);
  }

  function skip(seconds: number) {
    const waveform = waveSurferRef.current;
    if (!waveform) return;
    const duration = Math.max(decodedDurationMs, durationMs, 1) / 1000;
    waveform.setTime(Math.min(Math.max(0, waveform.getCurrentTime() + seconds), duration));
  }

  function handleKeyboard(event: KeyboardEvent<HTMLDivElement>) {
    if (event.target !== event.currentTarget || !ready) return;
    if (event.key === " " || event.key === "k") {
      event.preventDefault();
      togglePlayback();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      skip(-5);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      skip(5);
    }
  }

  const effectiveDurationMs = Math.max(decodedDurationMs, durationMs, 1);

  return (
    <section
      className="waveform-timeline"
      aria-label={`${label} waveform and transport`}
      tabIndex={0}
      onKeyDown={handleKeyboard}
    >
      <div className="waveform-stage">
        <div
          ref={containerRef}
          className="waveform-canvas"
          role="img"
          aria-label={`${label} audio waveform`}
        />
        <div className="waveform-word-markers" aria-label="Timed word markers">
          {words.map((word) => {
            const left = (word.start_ms / effectiveDurationMs) * 100;
            const width = ((word.end_ms - word.start_ms) / effectiveDurationMs) * 100;
            return (
              <button
                type="button"
                key={word.id}
                className={activeWordId === word.id ? "is-active" : ""}
                style={{ left: `${left}%`, width: `${Math.max(width, 0.35)}%` }}
                aria-label={`${word.text}, starts at ${formatTimelineTime(word.start_ms)}`}
                aria-pressed={activeWordId === word.id}
                title={`${word.text} · ${formatTimelineTime(word.start_ms)}`}
                onClick={() => {
                  onSelectWord(word.id);
                  seekToMs(word.start_ms, true);
                }}
              />
            );
          })}
        </div>
        {!ready && !waveformError && (
          <div className="waveform-loading" role="status">
            Drawing waveform{loading > 0 ? ` · ${loading}%` : "…"}
          </div>
        )}
      </div>

      {waveformError ? (
        <div className="waveform-fallback">
          <span>Waveform unavailable</span>
          <audio controls preload="metadata" src={src} aria-label={`${label} source audio`} />
        </div>
      ) : (
        <div className="transport-controls">
          <button
            type="button"
            aria-label="Back 5 seconds"
            disabled={!ready}
            onClick={() => skip(-5)}
          >
            −5s
          </button>
          <button
            type="button"
            className="transport-play"
            aria-label={playing ? "Pause audio" : "Play audio"}
            disabled={!ready}
            onClick={togglePlayback}
          >
            {playing ? "Pause" : "Play"}
          </button>
          <button
            type="button"
            aria-label="Forward 5 seconds"
            disabled={!ready}
            onClick={() => skip(5)}
          >
            +5s
          </button>
          <span className="transport-time" aria-live="off">
            {formatTimelineTime(positionMs)} / {formatTimelineTime(effectiveDurationMs)}
          </span>
          <input
            className="transport-position"
            type="range"
            min={0}
            max={effectiveDurationMs}
            step={10}
            value={Math.min(positionMs, effectiveDurationMs)}
            disabled={!ready}
            aria-label="Timeline position"
            onChange={(event) => seekToMs(Number(event.target.value))}
          />
          <label className="transport-rate">
            <span>Speed</span>
            <select
              value={playbackRate}
              disabled={!ready}
              aria-label="Playback speed"
              onChange={(event) => {
                const rate = Number(event.target.value);
                setPlaybackRate(rate);
                waveSurferRef.current?.setPlaybackRate(rate, true);
              }}
            >
              <option value={0.75}>0.75×</option>
              <option value={1}>1×</option>
              <option value={1.25}>1.25×</option>
              <option value={1.5}>1.5×</option>
            </select>
          </label>
        </div>
      )}
      <p className="transport-hint">Focus the timeline: Space/K plays, arrows seek 5 seconds.</p>
    </section>
  );
});

function formatTimelineTime(milliseconds: number) {
  const totalSeconds = Math.max(0, milliseconds) / 1000;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds - minutes * 60;
  return `${minutes}:${seconds.toFixed(1).padStart(4, "0")}`;
}
