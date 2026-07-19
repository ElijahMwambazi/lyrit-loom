import { describe, expect, it } from "vitest";

import {
  mergeCueWithPrevious,
  nudgeWord,
  setCueBounds,
  splitCue,
  validateTimeline,
  type EditorCue,
} from "./transcriptEditor";

const cues: EditorCue[] = [
  {
    id: "00000000-0000-4000-8000-000000000001",
    start_ms: 0,
    end_ms: 1200,
    words: [
      {
        id: "00000000-0000-4000-8000-000000000002",
        text: "Weave",
        start_ms: 0,
        end_ms: 500,
        confidence: 0.99,
      },
      {
        id: "00000000-0000-4000-8000-000000000003",
        text: "motion",
        start_ms: 600,
        end_ms: 1200,
        confidence: 0.9,
      },
    ],
  },
];

describe("transcript editor commands", () => {
  it("splits before a word and merges the resulting adjacent cues", () => {
    const split = splitCue(
      cues,
      cues[0]!.id,
      cues[0]!.words[1]!.id,
      () => "00000000-0000-4000-8000-000000000004",
    );

    expect(split).toHaveLength(2);
    expect(split[0]).toMatchObject({ end_ms: 500, words: [{ text: "Weave" }] });
    expect(split[1]).toMatchObject({
      id: "00000000-0000-4000-8000-000000000004",
      start_ms: 600,
      words: [{ text: "motion" }],
    });
    expect(validateTimeline(split, 4200)).toEqual([]);

    expect(mergeCueWithPrevious(split, split[1]!.id)).toEqual(cues);
  });

  it("updates cue bounds and reports invalid word containment", () => {
    const invalid = setCueBounds(cues, cues[0]!.id, 100, 1100);
    const issues = validateTimeline(invalid, 4200);

    expect(issues).toHaveLength(2);
    expect(issues.every((issue) => issue.wordId)).toBe(true);
  });

  it("nudges a word as a bounded unit and detects overlap", () => {
    const shifted = nudgeWord(cues, cues[0]!.words[1]!.id, -200, 4200);
    expect(shifted[0]!.words[1]).toMatchObject({ start_ms: 400, end_ms: 1000 });
    expect(validateTimeline(shifted, 4200)[0]?.message).toContain("word 2 timing");

    const bounded = nudgeWord(cues, cues[0]!.words[0]!.id, -500, 4200);
    expect(bounded[0]!.words[0]).toMatchObject({ start_ms: 0, end_ms: 500 });
  });
});
