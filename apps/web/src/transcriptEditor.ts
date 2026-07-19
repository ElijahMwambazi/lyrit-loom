import type { components } from "@lyrit/api-client";

type Transcript = components["schemas"]["TranscriptRevision"];
export type EditorCue = Transcript["cues"][number];

export type TimelineIssue = {
  message: string;
  cueId?: string;
  wordId?: string;
};

export function setCueBounds(
  cues: EditorCue[],
  cueId: string,
  startMs: number,
  endMs: number,
) {
  return cues.map((cue) =>
    cue.id === cueId ? { ...cue, start_ms: startMs, end_ms: endMs } : cue,
  );
}

export function splitCue(
  cues: EditorCue[],
  cueId: string,
  firstRightWordId: string,
  createId: () => string = () => crypto.randomUUID(),
) {
  const cueIndex = cues.findIndex((cue) => cue.id === cueId);
  if (cueIndex < 0) return cues;
  const cue = cues[cueIndex]!;
  const wordIndex = cue.words.findIndex((word) => word.id === firstRightWordId);
  if (wordIndex <= 0) return cues;

  const leftWords = cue.words.slice(0, wordIndex);
  const rightWords = cue.words.slice(wordIndex);
  const left: EditorCue = {
    ...cue,
    end_ms: leftWords[leftWords.length - 1]!.end_ms,
    words: leftWords,
  };
  const right: EditorCue = {
    ...cue,
    id: createId(),
    start_ms: rightWords[0]!.start_ms,
    words: rightWords,
  };
  return [...cues.slice(0, cueIndex), left, right, ...cues.slice(cueIndex + 1)];
}

export function mergeCueWithPrevious(cues: EditorCue[], cueId: string) {
  const cueIndex = cues.findIndex((cue) => cue.id === cueId);
  if (cueIndex <= 0) return cues;
  const left = cues[cueIndex - 1]!;
  const right = cues[cueIndex]!;
  const merged: EditorCue = {
    ...left,
    end_ms: right.end_ms,
    words: [...left.words, ...right.words],
  };
  return [
    ...cues.slice(0, cueIndex - 1),
    merged,
    ...cues.slice(cueIndex + 1),
  ];
}

export function nudgeWord(
  cues: EditorCue[],
  wordId: string,
  deltaMs: number,
  durationMs: number,
) {
  return cues.map((cue) => ({
    ...cue,
    words: cue.words.map((word) => {
      if (word.id !== wordId) return word;
      const boundedDelta = Math.max(
        -word.start_ms,
        Math.min(deltaMs, durationMs - word.end_ms),
      );
      return {
        ...word,
        start_ms: word.start_ms + boundedDelta,
        end_ms: word.end_ms + boundedDelta,
      };
    }),
  }));
}

export function validateTimeline(cues: EditorCue[], durationMs: number) {
  const issues: TimelineIssue[] = [];
  if (cues.length === 0) {
    return [{ message: "The transcript must contain at least one cue." }];
  }

  let previousCueEnd = 0;
  cues.forEach((cue, cueIndex) => {
    const cueLabel = `Cue ${cueIndex + 1}`;
    if (cue.start_ms < 0 || cue.end_ms <= cue.start_ms || cue.end_ms > durationMs) {
      issues.push({
        cueId: cue.id,
        message: `${cueLabel} bounds must be ordered and within the audio duration.`,
      });
    }
    if (cue.start_ms < previousCueEnd) {
      issues.push({
        cueId: cue.id,
        message: `${cueLabel} overlaps the previous cue.`,
      });
    }
    if (cue.words.length === 0) {
      issues.push({ cueId: cue.id, message: `${cueLabel} must contain a word.` });
    }

    let previousWordEnd = cue.start_ms;
    cue.words.forEach((word, wordIndex) => {
      const wordLabel = `${cueLabel}, word ${wordIndex + 1}`;
      if (word.text.trim().length === 0 || [...word.text].length > 200) {
        issues.push({
          cueId: cue.id,
          wordId: word.id,
          message: `${wordLabel} text must contain 1–200 characters.`,
        });
      }
      if (
        word.start_ms < previousWordEnd ||
        word.end_ms <= word.start_ms ||
        word.start_ms < cue.start_ms ||
        word.end_ms > cue.end_ms
      ) {
        issues.push({
          cueId: cue.id,
          wordId: word.id,
          message: `${wordLabel} timing must be ordered and inside its cue.`,
        });
      }
      previousWordEnd = word.end_ms;
    });
    previousCueEnd = cue.end_ms;
  });
  return issues;
}
