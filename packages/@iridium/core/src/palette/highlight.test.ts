/**
 * Tests for match-offset segmentation. Run with `bun test`.
 *
 * The astral cases are the reason this is shared code rather than a loop in each
 * face: a segmenter written against "one offset, one code unit" passes every
 * ASCII test and mangles the first emoji it meets.
 */

import { describe, expect, test } from "bun:test";
import { highlightSegments } from "./highlight.ts";

/** Rebuilds the input, which every segmentation must preserve exactly. */
function rejoin(text: string, matches?: readonly number[]): string {
  return highlightSegments(text, matches)
    .map((segment) => segment.text)
    .join("");
}

/** The matched runs alone, in order. */
function highlighted(text: string, matches?: readonly number[]): string[] {
  return highlightSegments(text, matches)
    .filter((segment) => segment.matched)
    .map((segment) => segment.text);
}

describe("segmenting", () => {
  test("no matches leaves one plain run", () => {
    expect(highlightSegments("Join Lines")).toEqual([
      { text: "Join Lines", matched: false },
    ]);
    expect(highlightSegments("Join Lines", [])).toEqual([
      { text: "Join Lines", matched: false },
    ]);
  });

  test("empty text produces no runs at all", () => {
    expect(highlightSegments("", [0])).toEqual([]);
  });

  test("a single match splits into three runs", () => {
    expect(highlightSegments("Join Lines", [5])).toEqual([
      { text: "Join ", matched: false },
      { text: "L", matched: true },
      { text: "ines", matched: false },
    ]);
  });

  test("adjacent matches coalesce into one run", () => {
    // A caller wraps each segment in an element; three <b>s where one belongs
    // breaks letter-spacing and reads as three separate hits.
    expect(highlightSegments("Join Lines", [5, 6, 7])).toEqual([
      { text: "Join ", matched: false },
      { text: "Lin", matched: true },
      { text: "es", matched: false },
    ]);
  });

  test("scattered matches stay separate", () => {
    expect(highlighted("Join Lines", [0, 5])).toEqual(["J", "L"]);
  });

  test("a match at each end needs no empty runs around it", () => {
    expect(highlightSegments("ab", [0, 1])).toEqual([{ text: "ab", matched: true }]);
    expect(highlightSegments("ab", [0])).toEqual([
      { text: "a", matched: true },
      { text: "b", matched: false },
    ]);
    expect(highlightSegments("ab", [1])).toEqual([
      { text: "a", matched: false },
      { text: "b", matched: true },
    ]);
  });
});

describe("characters outside the basic plane", () => {
  // "𝄞" is one character and two UTF-16 code units, so the kernel's offset for
  // the character after it is 2, not 1.
  const CLEF = "𝄞clef";

  test("an astral match takes both of its code units", () => {
    expect(highlightSegments(CLEF, [0])).toEqual([
      { text: "𝄞", matched: true },
      { text: "clef", matched: false },
    ]);
  });

  test("a match after an astral character lands where the kernel says", () => {
    expect(highlighted(CLEF, [2])).toEqual(["c"]);
  });

  test("an astral character and its neighbour coalesce", () => {
    expect(highlightSegments(CLEF, [0, 2])).toEqual([
      { text: "𝄞c", matched: true },
      { text: "lef", matched: false },
    ]);
  });

  test("no segmentation ever splits a surrogate pair", () => {
    // The failure this guards is silent: a lone surrogate is a valid JS string
    // and renders as a replacement glyph rather than throwing.
    for (const matches of [[0], [1], [2], [0, 1], [1, 2], [0, 2], [0, 1, 2, 3]]) {
      for (const segment of highlightSegments(CLEF, matches)) {
        // Round-tripping through code *points* drops nothing unless a pair was
        // broken, in which case the lone surrogate does not survive.
        expect([segment.text, matches]).toEqual([
          Array.from(segment.text).join(""),
          matches,
        ]);
      }
    }
  });

  test("a two-byte, one-unit character is one unit wide", () => {
    // "ü" is two bytes in UTF-8 and one UTF-16 unit; a segmenter written against
    // byte offsets highlights the wrong character here.
    expect(highlighted("Über", [0, 1])).toEqual(["Üb"]);
  });

  test("emoji survive a full highlight", () => {
    expect(highlightSegments("🎹🎺", [0, 2])).toEqual([{ text: "🎹🎺", matched: true }]);
  });
});

describe("malformed offsets", () => {
  test("an offset past the end is dropped, not clamped", () => {
    expect(highlightSegments("ab", [99])).toEqual([{ text: "ab", matched: false }]);
  });

  test("negative and fractional offsets are dropped", () => {
    expect(highlightSegments("abc", [-1, 1.5])).toEqual([{ text: "abc", matched: false }]);
  });

  test("an offset landing mid-pair is ignored rather than splitting it", () => {
    expect(highlightSegments("𝄞", [1])).toEqual([{ text: "𝄞", matched: false }]);
  });

  test("duplicate and unordered offsets give the same answer as clean ones", () => {
    expect(highlightSegments("Join Lines", [7, 5, 5, 6])).toEqual(
      highlightSegments("Join Lines", [5, 6, 7]),
    );
  });

  test("every input is reproduced exactly by concatenating the runs", () => {
    for (const text of ["", "a", "Join Lines", "𝄞clef", "🎹🎺", "Über"]) {
      for (const matches of [undefined, [], [0], [1], [0, 2], [-1, 99, 1]]) {
        expect(rejoin(text, matches)).toBe(text);
      }
    }
  });
});
