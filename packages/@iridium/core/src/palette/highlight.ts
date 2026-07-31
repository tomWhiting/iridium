/**
 * Turning match offsets into renderable runs.
 *
 * Every face has to draw the same thing — the characters a query matched, in
 * bold, inside the text they matched against — and every face would otherwise
 * write this loop itself. It lives here so React, the web component and any
 * future host highlight identically, and so it can be tested without a DOM.
 *
 * @module
 */

/** One run of text, either matched or not. Concatenating them rebuilds the input. */
export interface HighlightSegment {
  /** The run's text. */
  readonly text: string;
  /** Whether the query matched these characters. */
  readonly matched: boolean;
}

/**
 * Splits `text` into alternating matched and unmatched runs.
 *
 * `matches` are the UTF-16 offsets the kernel returns — one per matched
 * *character*, pointing at where that character starts. A character outside the
 * basic plane occupies **two** code units, so a run cannot be assumed one unit
 * wide: slicing on that assumption emits a lone surrogate, which renders as a
 * replacement glyph. Each offset is therefore widened to the whole character.
 *
 * Adjacent matches coalesce, so a consecutive run is one segment rather than
 * several, and a caller can wrap each segment in a single element.
 *
 * Offsets that are negative, fractional, past the end, or land mid-surrogate are
 * dropped rather than clamped — none can arise from a match against this same
 * text, and a highlight in the wrong place is worse than a missing one.
 */
export function highlightSegments(
  text: string,
  matches?: readonly number[],
): HighlightSegment[] {
  if (text.length === 0) {
    return [];
  }
  if (matches === undefined || matches.length === 0) {
    return [{ text, matched: false }];
  }

  const marked = new Uint8Array(text.length);
  for (const offset of matches) {
    if (!Number.isInteger(offset) || offset < 0 || offset >= text.length) {
      continue;
    }
    const unit = text.charCodeAt(offset);
    if (unit >= 0xdc00 && unit <= 0xdfff) {
      // A trailing surrogate is not where a character starts. The kernel never
      // emits one; marking it anyway would split the pair it belongs to.
      continue;
    }
    marked[offset] = 1;
    if (unit >= 0xd800 && unit <= 0xdbff && offset + 1 < text.length) {
      marked[offset + 1] = 1;
    }
  }

  const segments: HighlightSegment[] = [];
  let start = 0;
  let matched = marked[0] === 1;
  for (let index = 1; index <= text.length; index++) {
    const here = index < text.length && marked[index] === 1;
    if (index === text.length || here !== matched) {
      segments.push({ text: text.slice(start, index), matched });
      start = index;
      matched = here;
    }
  }
  return segments;
}
