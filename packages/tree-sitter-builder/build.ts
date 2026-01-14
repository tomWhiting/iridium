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
 *   bun run build        # Build all grammars
 *   bun run build:quick  # Build only core grammars (faster)
 */

import { spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  statSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

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

const CACHE_DIR = join(__dirname, ".cache");
const OUTPUT_DIR = join(__dirname, "../../crates/iridium-bindings/ts/syntax");

// web-tree-sitter version - must match the version in package.json
const WEB_TREE_SITTER_VERSION = "0.25.6";

interface BuildResult {
  lang: string;
  wasmPath: string;
  query: string;
  size: number;
}

function run(
  cmd: string,
  args: string[],
  cwd?: string
): Promise<{ success: boolean; output: string }> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, {
      cwd,
      stdio: ["inherit", "pipe", "pipe"],
    });

    let output = "";
    child.stdout?.on("data", (data) => {
      output += data.toString();
    });
    child.stderr?.on("data", (data) => {
      output += data.toString();
    });

    child.on("close", (code) => {
      resolve({ success: code === 0, output });
    });

    child.on("error", (err) => {
      resolve({ success: false, output: err.message });
    });
  });
}

async function cloneOrUpdate(name: string, repo: string): Promise<string> {
  const repoDir = join(CACHE_DIR, name);

  if (existsSync(repoDir)) {
    console.log(`  ♻️  Updating ${name}...`);
    const result = await run("git", ["pull", "--ff-only"], repoDir);
    if (!result.success) {
      // If pull fails, delete and re-clone
      rmSync(repoDir, { recursive: true });
      return cloneOrUpdate(name, repo);
    }
  } else {
    console.log(`  📥 Cloning ${name}...`);
    const result = await run("git", ["clone", "--depth", "1", repo, repoDir]);
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
  const buildDir = subPath ? join(repoDir, subPath) : repoDir;
  const wasmName = `tree-sitter-${lang}.wasm`;

  console.log(`  🔨 Building ${lang} WASM...`);

  const result = await run("tree-sitter", ["build", "--wasm"], buildDir);
  if (!result.success) {
    throw new Error(`Failed to build ${lang}: ${result.output}`);
  }

  // Find the generated WASM file
  const possibleNames = [
    wasmName,
    `tree-sitter-${subPath || lang}.wasm`,
  ];

  for (const name of possibleNames) {
    const wasmPath = join(buildDir, name);
    if (existsSync(wasmPath)) {
      return wasmPath;
    }
  }

  // Search for any .wasm file in the build dir
  const entries = readdirSync(buildDir);
  for (const entry of entries) {
    if (entry.endsWith(".wasm")) {
      return join(buildDir, entry);
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
    def.subPath ? join(repoDir, def.subPath, "queries/highlights.scm") : undefined,
    join(repoDir, "queries/highlights.scm"),
    join(repoDir, "queries/nvim/highlights.scm"),
    join(repoDir, "queries/helix/highlights.scm"),
  ].filter(Boolean) as string[];

  for (const queryPath of queryPaths) {
    if (existsSync(queryPath)) {
      console.log(`  📄 Found query for ${lang}`);
      return readFileSync(queryPath, "utf-8");
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

  const stat = statSync(wasmPath);

  return {
    lang,
    wasmPath,
    query,
    size: stat.size,
  };
}

async function downloadCoreWasm(): Promise<Uint8Array> {
  const url = `https://cdn.jsdelivr.net/npm/web-tree-sitter@${WEB_TREE_SITTER_VERSION}/tree-sitter.wasm`;
  console.log(`\n📥 Downloading tree-sitter core from ${url}...`);

  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to download core WASM: ${response.status}`);
  }

  const data = new Uint8Array(await response.arrayBuffer());
  console.log(`  ✅ Downloaded ${(data.length / 1024).toFixed(1)} KB`);
  return data;
}

function generateCoreTs(coreWasm: Uint8Array): string {
  const base64 = Buffer.from(coreWasm).toString("base64");

  return `/**
 * Auto-generated tree-sitter core WASM bundle.
 * Generated by: packages/tree-sitter-builder
 * Date: ${new Date().toISOString()}
 * Version: web-tree-sitter@${WEB_TREE_SITTER_VERSION}
 *
 * DO NOT EDIT MANUALLY
 */

const CORE_WASM_BASE64 = "${base64}";

/**
 * Decode the tree-sitter core WASM from base64.
 */
export function decodeCoreWasm(): Uint8Array {
  const binary = atob(CORE_WASM_BASE64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
`;
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
    const wasmData = readFileSync(result.wasmPath);
    const base64 = Buffer.from(wasmData).toString("base64");
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
  const args = process.argv.slice(2);
  const quickMode = args.includes("--quick");

  console.log("🌳 Tree-sitter Grammar Builder");
  console.log("==============================\n");

  // Check prerequisites
  const treeSitterCheck = await run("tree-sitter", ["--version"]);
  if (!treeSitterCheck.success) {
    console.error("❌ tree-sitter CLI not found!");
    console.error("   Install with: cargo install tree-sitter-cli");
    process.exit(1);
  }
  console.log(`✅ tree-sitter CLI: ${treeSitterCheck.output.trim()}`);

  // Setup directories
  mkdirSync(CACHE_DIR, { recursive: true });
  mkdirSync(OUTPUT_DIR, { recursive: true });

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
    process.exit(1);
  }

  // Download core WASM
  const coreWasm = await downloadCoreWasm();

  // Generate output files
  console.log("\n📝 Generating output files...");

  const coreTs = generateCoreTs(coreWasm);
  const grammarsTs = generateGrammarsTs(results);
  const queriesTs = generateQueriesTs(results);

  writeFileSync(join(OUTPUT_DIR, "core.gen.ts"), coreTs);
  writeFileSync(join(OUTPUT_DIR, "grammars.gen.ts"), grammarsTs);
  writeFileSync(join(OUTPUT_DIR, "queries.ts"), queriesTs);

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

  console.log(`\n  Total grammar WASM size: ${(totalSize / 1024 / 1024).toFixed(2)} MB`);
  console.log(`  Core WASM size: ${(coreWasm.length / 1024).toFixed(1)} KB`);
  console.log(`\n  Output: ${OUTPUT_DIR}/core.gen.ts`);
  console.log(`  Output: ${OUTPUT_DIR}/grammars.gen.ts`);
  console.log(`  Output: ${OUTPUT_DIR}/queries.ts`);

  if (errors.length > 0) {
    console.log(`\n⚠️  Failed grammars: ${errors.join(", ")}`);
  }
}

main();
