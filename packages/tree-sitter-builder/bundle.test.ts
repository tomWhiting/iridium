/**
 * The encoder and the loader it emits must agree. Nothing else in this package
 * can be tested without a network — `build.ts` clones nineteen grammar
 * repositories — so this is the whole guard, and it is the half that broke.
 *
 * Every test here generates a module from known bytes, writes it, imports it,
 * and asks the *emitted* loader for the bytes back. Decoding with `gunzipSync`
 * instead would prove only that zlib is symmetric: it would pass just as well
 * against a loader that had been left uncompressed, which is exactly the
 * regression that shipped in January.
 */

import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import {
  decompressBody,
  encodePayload,
  generateCoreTs,
  generateGrammarsTs,
} from "./bundle.ts";

const DATE = "2026-08-08T00:00:00.000Z";
const VERSION = "0.25.6";

/** Silences the size report; the tests assert on bytes, not on stdout. */
const quiet = () => {};

const workDir = mkdtempSync(join(tmpdir(), "iridium-bundle-"));

afterAll(() => rmSync(workDir, { recursive: true, force: true }));

/**
 * Writes generated source to its own file so a test can import it, and so the
 * loader under test is the one that would ship rather than a copy of it.
 *
 * ⚠️ **Every module must be written before the first `import()` runs.** The
 * runtime resolves these against a listing of `workDir` taken when it first
 * looks there, so a file created after that point is reported missing however
 * plainly it exists on disk. Calling this at module scope — not inside a test —
 * is what keeps all of them ahead of that line. Each also needs its own name:
 * a specifier is cached for the life of the process, so a reused path would
 * silently hand back the first module.
 */
function generatedModule(name: string, source: string): string {
  const path = join(workDir, `${name}.ts`);
  writeFileSync(path, source);
  return pathToFileURL(path).href;
}

/**
 * Bytes that gzip cannot trivially collapse, so a bundle that skipped
 * compression is not accidentally the same size as one that did not. A run of
 * identical bytes would compress ~1000×, which would hide a size regression
 * behind an unrepresentative ratio.
 */
function payload(seed: number, length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  let state = seed >>> 0;
  for (let i = 0; i < length; i++) {
    // xorshift32 — deterministic, so a failure reproduces exactly.
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[i] = state & 0xff;
  }
  return bytes;
}

/** Wraps a base64 payload in the loader the generators emit around theirs. */
function loaderModule(base64: string): string {
  return `const DATA = "${base64}";
export async function load(): Promise<Uint8Array> {
${decompressBody("DATA")}
  return result;
}
`;
}

// ── Fixtures. All written here, at module scope, ahead of the first import.

const SAMPLE = payload(1, 4096);
const CORE_WASM = payload(7, 20_000);
const GRAMMARS = [
  { lang: "rust", wasm: payload(11, 30_000) },
  { lang: "json", wasm: payload(13, 8_000) },
];

const sampleModule = generatedModule("sample", loaderModule(encodePayload("t", SAMPLE, quiet)));
const emptyModule = generatedModule(
  "empty",
  loaderModule(encodePayload("t", new Uint8Array(0), quiet))
);
const coreSource = generateCoreTs(CORE_WASM, VERSION, DATE, quiet);
const coreModule = generatedModule("core", coreSource);
const grammarsSource = generateGrammarsTs(GRAMMARS, DATE, quiet);
const grammarsModule = generatedModule("grammars", grammarsSource);

describe("encodePayload", () => {
  test("round trips through the emitted loader, not just through zlib", async () => {
    const { load } = (await import(sampleModule)) as { load: () => Promise<Uint8Array> };
    expect(Array.from(await load())).toEqual(Array.from(SAMPLE));
  });

  test("emits base64 that is smaller than the raw bytes it encodes", () => {
    // Base64 costs 4/3, so gzip has to beat that before compression is a win at
    // all. Compressible input is the honest case here — a real wasm grammar
    // compresses ~9.7×, and the January regression showed up as exactly this
    // ratio going to 0.75×.
    const bytes = new Uint8Array(64 * 1024);
    for (let i = 0; i < bytes.length; i++) bytes[i] = i % 251;
    expect(encodePayload("t", bytes, quiet).length).toBeLessThan(bytes.length);
  });

  test("accepts an empty payload", async () => {
    const { load } = (await import(emptyModule)) as { load: () => Promise<Uint8Array> };
    expect((await load()).length).toBe(0);
  });
});

describe("generateCoreTs", () => {
  test("the emitted decodeCoreWasm returns the bytes that went in", async () => {
    const { decodeCoreWasm } = (await import(coreModule)) as {
      decodeCoreWasm: () => Promise<Uint8Array>;
    };
    expect(Array.from(await decodeCoreWasm())).toEqual(Array.from(CORE_WASM));
  });

  test("records the version and says it is compressed", () => {
    expect(coreSource).toContain(`web-tree-sitter@${VERSION}`);
    expect(coreSource).toContain("gzip-compressed");
    expect(coreSource).toContain("DO NOT EDIT MANUALLY");
  });
});

describe("generateGrammarsTs", () => {
  test("every grammar decodes back to its own bytes", async () => {
    const { decodeGrammar } = (await import(grammarsModule)) as {
      decodeGrammar: (lang: string) => Promise<Uint8Array | null>;
    };
    for (const { lang, wasm } of GRAMMARS) {
      const got = await decodeGrammar(lang);
      expect(got).not.toBeNull();
      // Two entries carrying two different payloads: a loader that read the
      // wrong key, or a generator that wrote one map entry, would still pass a
      // single-grammar check.
      expect(Array.from(got as Uint8Array)).toEqual(Array.from(wasm));
    }
  });

  test("an unknown language is null rather than an error", async () => {
    const { decodeGrammar } = (await import(grammarsModule)) as {
      decodeGrammar: (lang: string) => Promise<Uint8Array | null>;
    };
    expect(await decodeGrammar("nonesuch")).toBeNull();
  });

  test("AVAILABLE_LANGUAGES lists exactly what the map holds", async () => {
    const { AVAILABLE_LANGUAGES } = (await import(grammarsModule)) as {
      AVAILABLE_LANGUAGES: readonly string[];
    };
    expect(AVAILABLE_LANGUAGES).toEqual(["rust", "json"]);
  });
});

describe("the shipped bundle", () => {
  /**
   * The live artefact must still load through the loader this builder emits.
   * `grammars.gen.ts` is stamped `DO NOT EDIT MANUALLY` and was last written in
   * January, so nothing else in the repo would notice the builder's format
   * drifting away from the file it is supposed to produce.
   */
  test("decodes through the same loader shape the builder emits", async () => {
    const live = (await import(
      "../@iridium/core/src/syntax/grammars.gen.ts"
    )) as {
      AVAILABLE_LANGUAGES: readonly string[];
      decodeGrammar: (lang: string) => Promise<Uint8Array | null>;
    };

    expect(live.AVAILABLE_LANGUAGES.length).toBeGreaterThan(0);
    const bytes = await live.decodeGrammar(live.AVAILABLE_LANGUAGES[0]);
    expect(bytes).not.toBeNull();
    // `\0asm` — proof the payload was genuinely decompressed, not merely
    // base64-decoded into whatever the gzip container happens to start with.
    expect(Array.from((bytes as Uint8Array).subarray(0, 4))).toEqual([
      0x00, 0x61, 0x73, 0x6d,
    ]);
  });
});
