/**
 * UTF-16 <-> UTF-8 offset conversion at the tree-sitter / rope boundary.
 *
 * ## Why this exists
 *
 * `web-tree-sitter` (0.25.6, pinned here) parses a JavaScript string and
 * reports every offset — `node.startIndex` / `node.endIndex`, `Point.column`,
 * and the fields of `Tree.edit()` — in **UTF-16 code units**. The parser
 * feeds the grammar via a callback that returns `string.slice(index)` and
 * copies each chunk with Emscripten's `stringToUTF16`, reporting the chunk
 * length as `string.length`; every index the parser hands back is therefore a
 * UTF-16 code-unit position, not a byte. (Proven empirically: parsing
 * `"// \u{1F600}\nlet x = 1;"` reports the `let` keyword at `startIndex = 6`
 * — six UTF-16 units — where the UTF-8 byte offset would be 8.)
 *
 * The Rust core, by contrast, stores the document as a UTF-8 rope. Highlight
 * spans are indexed into that rope by **byte** offset (the renderer slices
 * `&content[start..end]`), and the incremental-parse edit info the controller
 * forwards (`WebEditor::takeLastEdit`) is expressed in **rope bytes** with
 * **byte columns**.
 *
 * On ASCII the two encodings coincide, so the mismatch was invisible. On any
 * non-ASCII content (an emoji in a comment, `é` in an identifier) the UTF-16
 * indices tree-sitter emits are smaller than the rope byte offsets the
 * renderer expects: colors drift and a span endpoint landing inside a
 * multi-byte sequence panics Rust's slice with "byte index is not a char
 * boundary". These functions convert at the boundary so the rope only ever
 * sees byte offsets and tree-sitter only ever sees UTF-16 offsets.
 *
 * The worker owns the JavaScript-string world, so it owns the conversion. All
 * functions here are pure (no worker/DOM state) and exhaustively unit-tested
 * in `encoding.test.ts`.
 */

import type { HighlightSpan, EditInfo } from "./protocol.ts";

/**
 * A tree-sitter position paired with its absolute UTF-16 index, produced by
 * {@link utf8OffsetsToPoints}. `row` and `column` follow tree-sitter's `Point`
 * convention (0-based line, 0-based UTF-16 code units from the line start).
 */
export interface PointWithIndex {
	/** Absolute offset in UTF-16 code units. */
	readonly index: number;
	/** 0-based line number. */
	readonly row: number;
	/** 0-based column, in UTF-16 code units from the start of the line. */
	readonly column: number;
}

/**
 * Fast, allocation-free test for whether a string is pure ASCII.
 *
 * When true, UTF-16 code-unit offsets and UTF-8 byte offsets are identical
 * everywhere in the string, so both conversions become the identity and the
 * callers below take a zero-work fast path. Returns as soon as it sees a
 * non-ASCII unit, so non-ASCII documents pay almost nothing here.
 */
export function isAllAscii(s: string): boolean {
	for (let i = 0; i < s.length; i++) {
		if (s.charCodeAt(i) > 0x7f) {
			return false;
		}
	}
	return true;
}

/**
 * Number of UTF-8 bytes and UTF-16 code units occupied by the code point that
 * begins at `s[i]`. Well-formed input advances by whole code points; an
 * unpaired surrogate is treated as a 3-byte replacement character (matching
 * `TextEncoder`) so the scan can never desync or loop.
 */
function codePointWidths(s: string, i: number, n: number): { bytes: number; units: number } {
	const code = s.charCodeAt(i);
	if (code < 0x80) {
		return { bytes: 1, units: 1 };
	}
	if (code < 0x800) {
		return { bytes: 2, units: 1 };
	}
	if (code >= 0xd800 && code <= 0xdbff) {
		// High surrogate: a following low surrogate makes an astral code point
		// (4 UTF-8 bytes, 2 UTF-16 units). A lone high surrogate is invalid and
		// encodes as U+FFFD (3 bytes, 1 unit).
		const next = i + 1 < n ? s.charCodeAt(i + 1) : 0;
		if (next >= 0xdc00 && next <= 0xdfff) {
			return { bytes: 4, units: 2 };
		}
		return { bytes: 3, units: 1 };
	}
	// BMP code point >= U+0800 (includes a lone low surrogate -> U+FFFD).
	return { bytes: 3, units: 1 };
}

/**
 * Convert a batch of UTF-16 code-unit offsets in `content` to UTF-8 byte
 * offsets, in a single forward scan.
 *
 * `sortedUtf16Offsets` must be sorted ascending. The returned array is
 * parallel to it (`result[i]` is the byte offset of `sortedUtf16Offsets[i]`).
 * Offsets at or past the end of the string clamp to the total byte length;
 * offsets that fall inside a surrogate pair clamp forward to the next code
 * point boundary (tree-sitter never emits such offsets, but the scan stays
 * robust if one appears).
 */
export function utf16OffsetsToUtf8(content: string, sortedUtf16Offsets: number[]): number[] {
	const out = new Array<number>(sortedUtf16Offsets.length);
	const n = content.length;
	let u16 = 0;
	let u8 = 0;
	let oi = 0;

	// Record every requested offset that has been reached at the current
	// position. `<=` (not `===`) guarantees an offset is never skipped even if
	// it lands mid-code-point.
	while (oi < sortedUtf16Offsets.length && sortedUtf16Offsets[oi] <= u16) {
		out[oi] = u8;
		oi++;
	}
	while (oi < sortedUtf16Offsets.length && u16 < n) {
		const { bytes, units } = codePointWidths(content, u16, n);
		u16 += units;
		u8 += bytes;
		while (oi < sortedUtf16Offsets.length && sortedUtf16Offsets[oi] <= u16) {
			out[oi] = u8;
			oi++;
		}
	}
	// Any offsets beyond the string clamp to the total byte length.
	while (oi < sortedUtf16Offsets.length) {
		out[oi] = u8;
		oi++;
	}
	return out;
}

/**
 * Convert a batch of UTF-8 byte offsets in `content` to tree-sitter points
 * (absolute UTF-16 index plus row/column), in a single forward scan.
 *
 * `sortedByteOffsets` must be sorted ascending. Row/column are recomputed from
 * `content` rather than trusted from the caller so that the index and the
 * point are guaranteed mutually consistent — tree-sitter requires this of the
 * arguments to `Tree.edit()`. Byte offsets at or past the end clamp to the end
 * of the string; offsets inside a multi-byte sequence clamp forward to the
 * next code point boundary.
 */
export function utf8OffsetsToPoints(content: string, sortedByteOffsets: number[]): PointWithIndex[] {
	const out = new Array<PointWithIndex>(sortedByteOffsets.length);
	const n = content.length;
	let u16 = 0;
	let u8 = 0;
	let row = 0;
	let rowStartU16 = 0;
	let oi = 0;

	const record = (): void => {
		while (oi < sortedByteOffsets.length && sortedByteOffsets[oi] <= u8) {
			out[oi] = { index: u16, row, column: u16 - rowStartU16 };
			oi++;
		}
	};

	record();
	while (oi < sortedByteOffsets.length && u16 < n) {
		const code = content.charCodeAt(u16);
		const { bytes, units } = codePointWidths(content, u16, n);
		u16 += units;
		u8 += bytes;
		if (code === 0x0a) {
			// The newline byte belongs to the line it terminates; the next
			// position starts a fresh row at column 0.
			row++;
			rowStartU16 = u16;
		}
		record();
	}
	// Any offsets beyond the string clamp to the final position.
	while (oi < sortedByteOffsets.length) {
		out[oi] = { index: u16, row, column: u16 - rowStartU16 };
		oi++;
	}
	return out;
}

/**
 * Rewrite highlight spans from tree-sitter's UTF-16 code-unit offsets to the
 * UTF-8 byte offsets the Rust rope indexes by.
 *
 * A single forward scan over the unique, sorted span endpoints does the whole
 * pass (O(n + k log k) for k endpoints), not one scan per span. Pure ASCII
 * documents return the input array unchanged after one cheap {@link isAllAscii}
 * probe. The `type` of each span is preserved; only `start`/`end` change.
 */
export function convertSpansToUtf8(content: string, spans: HighlightSpan[]): HighlightSpan[] {
	if (spans.length === 0 || isAllAscii(content)) {
		return spans;
	}

	const endpoints = new Set<number>();
	for (const span of spans) {
		endpoints.add(span.start);
		endpoints.add(span.end);
	}
	const sorted = [...endpoints].sort((a, b) => a - b);
	const bytes = utf16OffsetsToUtf8(content, sorted);

	const map = new Map<number, number>();
	for (let i = 0; i < sorted.length; i++) {
		map.set(sorted[i], bytes[i]);
	}

	return spans.map((span) => ({
		start: map.get(span.start) ?? span.start,
		end: map.get(span.end) ?? span.end,
		type: span.type,
	}));
}

/**
 * Rewrite Rust-provided edit info from rope bytes / byte columns to the UTF-16
 * code units and UTF-16 columns `Tree.edit()` requires.
 *
 * The incoming {@link EditInfo} carries rope-byte offsets in its `*Index`
 * fields and byte columns in its `*Position.column` fields — exactly the
 * shape `WebEditor::takeLastEdit` produces (see `JsEditInfo`). Conversion
 * needs both documents because the fields straddle the edit:
 *
 * - `startIndex` and `newEndIndex` are positions in the **post-edit** document
 *   (`newContent`).
 * - `oldEndIndex` is a position in the **pre-edit** document (`oldContent`) —
 *   the text the worker last parsed. `startByte <= newEndByte` always holds,
 *   so the two post-edit offsets can share one scan.
 *
 * On ASCII-only documents bytes and code units coincide, so the input is
 * already valid UTF-16 and is returned unchanged.
 */
export function convertEditInfo(
	oldContent: string,
	newContent: string,
	edit: EditInfo,
): EditInfo {
	if (isAllAscii(oldContent) && isAllAscii(newContent)) {
		return edit;
	}

	const [startPoint, newEndPoint] = utf8OffsetsToPoints(newContent, [
		edit.startIndex,
		edit.newEndIndex,
	]);
	const [oldEndPoint] = utf8OffsetsToPoints(oldContent, [edit.oldEndIndex]);

	return {
		startIndex: startPoint.index,
		oldEndIndex: oldEndPoint.index,
		newEndIndex: newEndPoint.index,
		startPosition: { row: startPoint.row, column: startPoint.column },
		oldEndPosition: { row: oldEndPoint.row, column: oldEndPoint.column },
		newEndPosition: { row: newEndPoint.row, column: newEndPoint.column },
	};
}
