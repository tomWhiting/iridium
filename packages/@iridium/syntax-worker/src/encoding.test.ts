/**
 * Tests for the UTF-16 <-> UTF-8 offset conversion at the tree-sitter / rope
 * boundary. Run with `bun test`.
 *
 * The oracle for every expectation is JavaScript's own encoders:
 * `str.length` for UTF-16 code units and `Buffer.byteLength(str, "utf8")` /
 * `TextEncoder` for UTF-8 bytes. The conversion functions must agree with
 * those exactly for any prefix of a string.
 */

import { describe, expect, test } from "bun:test";
import {
	isAllAscii,
	utf16OffsetsToUtf8,
	utf8OffsetsToPoints,
	convertSpansToUtf8,
	convertEditInfo,
	type PointWithIndex,
} from "./encoding.ts";
import type { HighlightSpan, EditInfo } from "./protocol.ts";

const enc = new TextEncoder();

/** UTF-8 byte length of the first `unit` UTF-16 code units of `s`. */
function utf8BytesOfPrefix(s: string, unit: number): number {
	return enc.encode(s.slice(0, unit)).length;
}

/**
 * Every code-point boundary of `s`, as parallel UTF-16 unit and UTF-8 byte
 * offsets. These are the only offsets tree-sitter (UTF-16) or the Rust rope
 * (UTF-8) can ever produce — neither splits a code point — so they are the
 * offsets the converters are contracted to round-trip exactly.
 */
function codePointBoundaries(s: string): Array<{ unit: number; byte: number }> {
	const res = [{ unit: 0, byte: 0 }];
	let unit = 0;
	let byte = 0;
	for (const ch of s) {
		unit += ch.length; // 1 (BMP) or 2 (surrogate pair) UTF-16 units
		byte += enc.encode(ch).length;
		res.push({ unit, byte });
	}
	return res;
}

describe("isAllAscii", () => {
	test("true for empty and pure ASCII", () => {
		expect(isAllAscii("")).toBe(true);
		expect(isAllAscii("let x = 1;\nfn main() {}")).toBe(true);
		expect(isAllAscii("")).toBe(true); // DEL is the last ASCII unit
	});

	test("false when any non-ASCII unit is present", () => {
		expect(isAllAscii("café")).toBe(false); // é
		expect(isAllAscii("a\u{1F600}b")).toBe(false); // emoji
		expect(isAllAscii("")).toBe(false); // first non-ASCII unit
	});
});

describe("utf16OffsetsToUtf8", () => {
	test("emoji before a keyword: `// 😀\\nlet` maps unit 6 -> byte 8", () => {
		const s = "// \u{1F600}\nlet x = 1;";
		// Oracle: UTF-16 length 16, UTF-8 length 18; `let` at unit 6 / byte 8.
		expect(s.length).toBe(16);
		expect(enc.encode(s).length).toBe(18);
		const out = utf16OffsetsToUtf8(s, [0, 3, 5, 6, s.length]);
		expect(out).toEqual([0, 3, 7, 8, 18]);
	});

	test("é inside an identifier maps every prefix like TextEncoder", () => {
		const s = "let café = 1;";
		const offsets = Array.from({ length: s.length + 1 }, (_, i) => i);
		const out = utf16OffsetsToUtf8(s, offsets);
		for (let unit = 0; unit <= s.length; unit++) {
			expect(out[unit]).toBe(utf8BytesOfPrefix(s, unit));
		}
	});

	test("multi-line non-ASCII: matches oracle at every code-point boundary", () => {
		const s = "// \u{1F4A9} first\nconst é = 世界;\nlet z = 2;";
		const bounds = codePointBoundaries(s);
		const out = utf16OffsetsToUtf8(s, bounds.map((b) => b.unit));
		for (let i = 0; i < bounds.length; i++) {
			expect(out[i]).toBe(bounds[i].byte);
		}
	});

	test("endpoints at string boundaries clamp correctly", () => {
		const s = "\u{1F600}éx"; // astral(4B/2u) + é(2B/1u) + x(1B/1u)
		// offset 0 -> 0; past-the-end (and beyond) -> total byte length (7)
		const out = utf16OffsetsToUtf8(s, [0, s.length, s.length + 5]);
		expect(out[0]).toBe(0);
		expect(out[1]).toBe(enc.encode(s).length);
		expect(out[2]).toBe(enc.encode(s).length);
	});
});

describe("utf8OffsetsToPoints", () => {
	test("byte offsets map to UTF-16 index and (row, column)", () => {
		const s = "// \u{1F600}\nlet x = 1;";
		// `let` begins at byte 8 -> unit 6, row 1, column 0.
		const [p] = utf8OffsetsToPoints(s, [8]);
		expect(p).toEqual({ index: 6, row: 1, column: 0 });
	});

	test("column is measured in UTF-16 units from the line start", () => {
		// Row 1: `x = café` — the `=` sits after `x ` (2 units).
		const s = "a\nx = café";
		const eqByte = enc.encode("a\nx ").length; // byte offset of `=`
		const [p] = utf8OffsetsToPoints(s, [eqByte]);
		expect(p.row).toBe(1);
		expect(p.column).toBe(2);
		expect(p.index).toBe("a\nx ".length);
	});

	test("index/row/column agree with the oracle at every code-point boundary", () => {
		const s = "// \u{1F4A9}\nconst é = 世;\nend";
		const bounds = codePointBoundaries(s);
		const pts = utf8OffsetsToPoints(s, bounds.map((b) => b.byte));
		for (let i = 0; i < bounds.length; i++) {
			const p = pts[i];
			expect(p.index).toBe(bounds[i].unit);
			// Derive expected row/column from the UTF-16 prefix.
			const prefix = s.slice(0, p.index);
			const nl = prefix.lastIndexOf("\n");
			const expectedRow = (prefix.match(/\n/g) ?? []).length;
			const expectedCol = nl === -1 ? prefix.length : prefix.length - (nl + 1);
			expect(p.row).toBe(expectedRow);
			expect(p.column).toBe(expectedCol);
		}
	});
});

describe("convertSpansToUtf8", () => {
	test("ASCII fast path returns the same array reference (zero work)", () => {
		const spans: HighlightSpan[] = [{ start: 0, end: 3, type: "keyword" }];
		expect(convertSpansToUtf8("let x = 1;", spans)).toBe(spans);
	});

	test("emoji-in-comment then keyword: keyword span shifts to byte offsets", () => {
		const s = "// \u{1F600}\nlet x = 1;";
		// tree-sitter (UTF-16) spans: comment [0,5], `let` keyword [6,9].
		const spans: HighlightSpan[] = [
			{ start: 0, end: 5, type: "comment" },
			{ start: 6, end: 9, type: "keyword" },
		];
		const out = convertSpansToUtf8(s, spans);
		expect(out).toEqual([
			{ start: 0, end: 7, type: "comment" }, // `// 😀` is 7 UTF-8 bytes
			{ start: 8, end: 11, type: "keyword" }, // `let`
		]);
		// The sliced bytes must reproduce the original substrings.
		const b = enc.encode(s);
		const dec = new TextDecoder();
		expect(dec.decode(b.slice(8, 11))).toBe("let");
	});

	test("é inside an identifier: identifier span uses byte length", () => {
		const s = "let café = 1;";
		// UTF-16 identifier span [4,8] -> bytes [4,9] (é is 2 bytes).
		const spans: HighlightSpan[] = [{ start: 4, end: 8, type: "variable" }];
		const out = convertSpansToUtf8(s, spans);
		expect(out).toEqual([{ start: 4, end: 9, type: "variable" }]);
	});

	test("span endpoints exactly at string boundaries", () => {
		const s = "é\u{1F600}";
		const spans: HighlightSpan[] = [{ start: 0, end: s.length, type: "all" }];
		const out = convertSpansToUtf8(s, spans);
		expect(out).toEqual([{ start: 0, end: enc.encode(s).length, type: "all" }]);
	});
});

describe("convertEditInfo", () => {
	/** Build byte-encoded edit info the way the controller does from Rust. */
	function byteEdit(
		startByte: number,
		oldEndByte: number,
		newEndByte: number,
		start: [number, number],
		oldEnd: [number, number],
		newEnd: [number, number],
	): EditInfo {
		return {
			startIndex: startByte,
			oldEndIndex: oldEndByte,
			newEndIndex: newEndByte,
			startPosition: { row: start[0], column: start[1] },
			oldEndPosition: { row: oldEnd[0], column: oldEnd[1] },
			newEndPosition: { row: newEnd[0], column: newEnd[1] },
		};
	}

	test("ASCII documents pass through unchanged", () => {
		const e = byteEdit(4, 4, 5, [0, 4], [0, 4], [0, 5]);
		expect(convertEditInfo("let  = 1;", "let x = 1;", e)).toBe(e);
	});

	test("insertion after a non-ASCII identifier converts bytes -> UTF-16", () => {
		// Pre-edit:  `let café = 1;`   Post-edit: `let caféx = 1;`
		// `x` inserted at byte 9 (right after café's é, which ends at byte 9).
		const oldContent = "let café = 1;";
		const newContent = "let caféx = 1;";
		const insByte = enc.encode("let café").length; // 9
		// startByte == oldEndByte == insByte; newEndByte == insByte + 1 ("x").
		const e = byteEdit(
			insByte,
			insByte,
			insByte + 1,
			[0, insByte],
			[0, insByte],
			[0, insByte + 1],
		);
		const out = convertEditInfo(oldContent, newContent, e);
		// é ends at UTF-16 unit 8 in both strings, so the insert point is unit 8.
		expect(out.startIndex).toBe(8);
		expect(out.oldEndIndex).toBe(8);
		expect(out.newEndIndex).toBe(9); // one unit for `x`
		expect(out.startPosition).toEqual({ row: 0, column: 8 });
		expect(out.oldEndPosition).toEqual({ row: 0, column: 8 });
		expect(out.newEndPosition).toEqual({ row: 0, column: 9 });
	});

	test("deletion of a multi-byte char shrinks oldEnd against the pre-edit text", () => {
		// Pre-edit: `x = 😀y`  Post-edit: `x = y`  (emoji deleted)
		const oldContent = "x = \u{1F600}y";
		const newContent = "x = y";
		const delByte = enc.encode("x = ").length; // 4
		const emojiBytes = enc.encode("\u{1F600}").length; // 4
		// startByte 4; oldEndByte 4+4=8 (pre-edit); newEndByte 4 (nothing inserted).
		const e = byteEdit(
			delByte,
			delByte + emojiBytes,
			delByte,
			[0, delByte],
			[0, delByte + emojiBytes],
			[0, delByte],
		);
		const out = convertEditInfo(oldContent, newContent, e);
		expect(out.startIndex).toBe(4); // unit 4 in both
		// Pre-edit oldEnd: after `x = 😀` = 4 + 2 surrogate units = 6.
		expect(out.oldEndIndex).toBe(6);
		expect(out.oldEndPosition).toEqual({ row: 0, column: 6 });
		expect(out.newEndIndex).toBe(4);
		expect(out.newEndPosition).toEqual({ row: 0, column: 4 });
	});

	test("roundtrip: apply the converted edit to the tree-sitter index space", () => {
		// A multi-line, multi-byte edit: replace `世` with `AB` on line 1.
		//   old line 1: `x = 世;`   new line 1: `x = AB;`
		const oldContent = "// c\nx = 世;";
		const newContent = "// c\nx = AB;";
		const startByte = enc.encode("// c\nx = ").length; // byte before 世
		const oldEndByte = startByte + enc.encode("世").length; // after 世 (3 bytes)
		const newEndByte = startByte + enc.encode("AB").length; // after AB (2 bytes)
		const startCol = "x = ".length;
		const e = byteEdit(
			startByte,
			oldEndByte,
			newEndByte,
			[1, startCol],
			[1, startCol + 1], // 世 is 1 UTF-16 unit (byte column is irrelevant post-convert)
			[1, startCol + 2],
		);
		const out = convertEditInfo(oldContent, newContent, e);
		// Independently verify against the oracle.
		const startIdx = "// c\nx = ".length;
		expect(out.startIndex).toBe(startIdx);
		expect(out.startPosition).toEqual({ row: 1, column: startCol });
		expect(out.oldEndIndex).toBe(startIdx + 1); // 世 = 1 UTF-16 unit
		expect(out.oldEndPosition).toEqual({ row: 1, column: startCol + 1 });
		expect(out.newEndIndex).toBe(startIdx + 2); // AB = 2 UTF-16 units
		expect(out.newEndPosition).toEqual({ row: 1, column: startCol + 2 });
	});
});

// Keep the exported Point type referenced so `bun test` type-checks the import.
const _pointTypeCheck: PointWithIndex = { index: 0, row: 0, column: 0 };
void _pointTypeCheck;
