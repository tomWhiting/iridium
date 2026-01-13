/**
 * Tree-sitter Grammar Builder
 *
 * Builds latest tree-sitter grammar WASMs and extracts matching queries.
 * Outputs bundled files to iridium-bindings/ts/syntax/
 *
 * Requirements:
 * - tree-sitter CLI installed (cargo install tree-sitter-cli)
 * - git available
 * - Emscripten OR Docker for WASM compilation
 *
 * Usage:
 *   deno task build        # Build all grammars
 *   deno task build:quick  # Build only core grammars (faster)
 */

import { encodeBase64 } from "jsr:@std/encoding@1/base64";
import { ensureDir } from "jsr:@std/fs@1/ensure-dir";
import { exists } from "jsr:@std/fs@1/exists";

// Grammar definitions - repo URL and any special handling
interface GrammarDef {
  repo: string;
  subPath?: string; // For monorepos like typescript
  queryFile?: string; // Custom query path if not queries/highlights.scm
}

const GRAMMARS: Record<string, GrammarDef> = {
  // Core languages (--quick flag)
  rust: { repo: "https://github.com/tree-sitter/tree-sitter-rust" },
  typescript: {
    repo: "https://github.com/tree-sitter/tree-sitter-typescript",
    subPath: "typescript",
  },
  tsx: {
    repo: "https://github.com/tree-sitter/tree-sitter-typescript",
    subPath: "tsx",
  },
  javascript: {
    repo: "https://github.com/tree-sitter/tree-sitter-javascript",
  },
  python: { repo: "https://github.com/tree-sitter/tree-sitter-python" },
  json: { repo: "https://github.com/tree-sitter/tree-sitter-json" },

  // Extended languages
  go: { repo: "https://github.com/tree-sitter/tree-sitter-go" },
  html: { repo: "https://github.com/tree-sitter/tree-sitter-html" },
  css: { repo: "https://github.com/tree-sitter/tree-sitter-css" },
  cpp: { repo: "https://github.com/tree-sitter/tree-sitter-cpp" },
  c: { repo: "https://github.com/tree-sitter/tree-sitter-c" },
  java: { repo: "https://github.com/tree-sitter/tree-sitter-java" },
  ruby: { repo: "https://github.com/tree-sitter/tree-sitter-ruby" },
  bash: { repo: "https://github.com/tree-sitter/tree-sitter-bash" },

  // Community grammars
  toml: { repo: "https://github.com/tree-sitter-grammars/tree-sitter-toml" },
  yaml: { repo: "https://github.com/tree-sitter-grammars/tree-sitter-yaml" },
  lua: { repo: "https://github.com/tree-sitter-grammars/tree-sitter-lua" },
  zig: { repo: "https://github.com/tree-sitter-grammars/tree-sitter-zig" },
  markdown: {
    repo: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
    subPath: "tree-sitter-markdown",
  },
};

const QUICK_GRAMMARS = [
  "rust",
  "typescript",
  "tsx",
  "javascript",
  "python",
  "json",
];

const SCRIPT_DIR = new URL(".", import.meta.url).pathname;
const CACHE_DIR = `${SCRIPT_DIR}.cache`;
const OUTPUT_DIR = `${SCRIPT_DIR}../../crates/iridium-bindings/ts/syntax`;

interface BuildResult {
  lang: string;
  wasmPath: string;
  query: string;
  size: number;
}

async function run(
  cmd: string[],
  cwd?: string
): Promise<{ success: boolean; output: string }> {
  const command = new Deno.Command(cmd[0], {
    args: cmd.slice(1),
    cwd,
    stdout: "piped",
    stderr: "piped",
  });
  const result = await command.output();
  const output =
    new TextDecoder().decode(result.stdout) +
    new TextDecoder().decode(result.stderr);
  return { success: result.success, output };
}

async function cloneOrUpdate(name: string, repo: string): Promise<string> {
  const repoDir = `${CACHE_DIR}/${name}`;

  if (await exists(repoDir)) {
    console.log(`  ♻️  Updating ${name}...`);
    const result = await run(["git", "pull", "--ff-only"], repoDir);
    if (!result.success) {
      // If pull fails, delete and re-clone
      await Deno.remove(repoDir, { recursive: true });
      return cloneOrUpdate(name, repo);
    }
  } else {
    console.log(`  📥 Cloning ${name}...`);
    const result = await run([
      "git",
      "clone",
      "--depth",
      "1",
      repo,
      repoDir,
    ]);
    if (!result.success) {
      throw new Error(`Failed to clone ${name}: ${result.output}`);
    }
  }

  return repoDir;
}

async function buildWasm(
  lang: string,
  repoDir: string,
  subPath?: string
): Promise<string> {
  const buildDir = subPath ? `${repoDir}/${subPath}` : repoDir;
  const wasmName = `tree-sitter-${lang}.wasm`;

  console.log(`  🔨 Building ${lang} WASM...`);

  const result = await run(["tree-sitter", "build", "--wasm"], buildDir);
  if (!result.success) {
    throw new Error(`Failed to build ${lang}: ${result.output}`);
  }

  // Find the generated WASM file
  const possibleNames = [
    wasmName,
    `tree-sitter-${subPath || lang}.wasm`,
    // Handle special cases like tree-sitter-typescript.wasm in typescript subdir
  ];

  for (const name of possibleNames) {
    const wasmPath = `${buildDir}/${name}`;
    if (await exists(wasmPath)) {
      return wasmPath;
    }
  }

  // Search for any .wasm file in the build dir
  for await (const entry of Deno.readDir(buildDir)) {
    if (entry.name.endsWith(".wasm")) {
      return `${buildDir}/${entry.name}`;
    }
  }

  throw new Error(`No WASM file found for ${lang} in ${buildDir}`);
}

async function extractQuery(
  lang: string,
  repoDir: string,
  def: GrammarDef
): Promise<string> {
  // Try multiple possible query locations
  const queryPaths = [
    def.queryFile,
    def.subPath
      ? `${repoDir}/${def.subPath}/queries/highlights.scm`
      : undefined,
    `${repoDir}/queries/highlights.scm`,
    // Some repos put queries in different places
    `${repoDir}/queries/nvim/highlights.scm`,
    `${repoDir}/queries/helix/highlights.scm`,
  ].filter(Boolean) as string[];

  for (const queryPath of queryPaths) {
    if (await exists(queryPath)) {
      console.log(`  📄 Found query for ${lang}`);
      return await Deno.readTextFile(queryPath);
    }
  }

  console.warn(`  ⚠️  No query found for ${lang}, using empty query`);
  return "";
}

async function buildGrammar(
  lang: string,
  def: GrammarDef
): Promise<BuildResult> {
  console.log(`\n📦 Processing ${lang}...`);

  // For monorepos (typescript/tsx), use same clone
  const repoName = def.repo.split("/").pop()!.replace(".git", "");
  const repoDir = await cloneOrUpdate(repoName, def.repo);
  const wasmPath = await buildWasm(lang, repoDir, def.subPath);
  const query = await extractQuery(lang, repoDir, def);

  const stat = await Deno.stat(wasmPath);

  return {
    lang,
    wasmPath,
    query,
    size: stat.size,
  };
}

function generateGrammarsTs(results: BuildResult[]): string {
  const languages = results.map((r) => r.lang);

  let code = `/**
 * Auto-generated tree-sitter grammar bundles.
 * Generated by: packages/tree-sitter-builder
 * Date: ${new Date().toISOString()}
 *
 * DO NOT EDIT MANUALLY
 */

export const AVAILABLE_LANGUAGES = [
${languages.map((l) => `  "${l}",`).join("\n")}
] as const;

const GRAMMAR_DATA: Record<string, string> = {
`;

  for (const result of results) {
    const wasmData = Deno.readFileSync(result.wasmPath);
    const base64 = encodeBase64(wasmData);
    code += `  "${result.lang}": "${base64}",\n`;
  }

  code += `};

/**
 * Decode a grammar from base64 to Uint8Array.
 */
export function decodeGrammar(lang: string): Uint8Array | null {
  const data = GRAMMAR_DATA[lang];
  if (!data) return null;

  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
`;

  return code;
}

function generateQueriesTs(results: BuildResult[]): string {
  let code = `/**
 * Auto-generated tree-sitter highlight queries.
 * Generated by: packages/tree-sitter-builder
 * Date: ${new Date().toISOString()}
 *
 * DO NOT EDIT MANUALLY
 */

export const HIGHLIGHT_QUERIES: Record<string, string> = {
`;

  for (const result of results) {
    // Escape the query string for JavaScript
    const escaped = result.query
      .replace(/\\/g, "\\\\")
      .replace(/`/g, "\\`")
      .replace(/\$/g, "\\$");
    code += `  "${result.lang}": \`${escaped}\`,\n\n`;
  }

  code += `};
`;

  return code;
}

async function main() {
  const args = Deno.args;
  const quickMode = args.includes("--quick");

  console.log("🌳 Tree-sitter Grammar Builder");
  console.log("==============================\n");

  // Check prerequisites
  const treeSitterCheck = await run(["tree-sitter", "--version"]);
  if (!treeSitterCheck.success) {
    console.error("❌ tree-sitter CLI not found!");
    console.error("   Install with: cargo install tree-sitter-cli");
    Deno.exit(1);
  }
  console.log(`✅ tree-sitter CLI: ${treeSitterCheck.output.trim()}`);

  // Setup directories
  await ensureDir(CACHE_DIR);
  await ensureDir(OUTPUT_DIR);

  // Select grammars to build
  const grammarsToBuild = quickMode
    ? Object.fromEntries(
        Object.entries(GRAMMARS).filter(([k]) => QUICK_GRAMMARS.includes(k))
      )
    : GRAMMARS;

  console.log(
    `\n📋 Building ${Object.keys(grammarsToBuild).length} grammars${quickMode ? " (quick mode)" : ""}...`
  );

  const results: BuildResult[] = [];
  const errors: string[] = [];

  for (const [lang, def] of Object.entries(grammarsToBuild)) {
    try {
      const result = await buildGrammar(lang, def);
      results.push(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error(`❌ Failed to build ${lang}: ${msg}`);
      errors.push(lang);
    }
  }

  if (results.length === 0) {
    console.error("\n❌ No grammars built successfully!");
    Deno.exit(1);
  }

  // Generate output files
  console.log("\n📝 Generating output files...");

  const grammarsTs = generateGrammarsTs(results);
  const queriesTs = generateQueriesTs(results);

  await Deno.writeTextFile(`${OUTPUT_DIR}/grammars.gen.ts`, grammarsTs);
  await Deno.writeTextFile(`${OUTPUT_DIR}/queries.ts`, queriesTs);

  // Summary
  console.log("\n✅ Build complete!");
  console.log("==================");

  let totalSize = 0;
  for (const result of results) {
    const sizeKB = (result.size / 1024).toFixed(1);
    const hasQuery = result.query.length > 0 ? "✓" : "✗";
    console.log(`  ${result.lang}: ${sizeKB} KB (query: ${hasQuery})`);
    totalSize += result.size;
  }

  console.log(`\n  Total WASM size: ${(totalSize / 1024 / 1024).toFixed(2)} MB`);
  console.log(`  Output: ${OUTPUT_DIR}/grammars.gen.ts`);
  console.log(`  Output: ${OUTPUT_DIR}/queries.ts`);

  if (errors.length > 0) {
    console.log(`\n⚠️  Failed grammars: ${errors.join(", ")}`);
  }
}

await main();
