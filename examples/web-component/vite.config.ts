import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  resolve: {
    alias: {
      // New @iridium packages (recommended)
      "@iridium/syntax-worker/worker": resolve(__dirname, "../../packages/@iridium/syntax-worker/src/worker.ts"),
      "@iridium/syntax-worker": resolve(__dirname, "../../packages/@iridium/syntax-worker/src/client.ts"),
      "@iridium/core/element": resolve(__dirname, "../../packages/@iridium/core/src/element/index.ts"),
      "@iridium/core/syntax": resolve(__dirname, "../../packages/@iridium/core/src/syntax/index.ts"),
      "@iridium/core": resolve(__dirname, "../../packages/@iridium/core/src/controller/index.ts"),
      // The wasm bundle. The `ts/` half of iridium-bindings was deleted with
      // #64 — a January fork of @iridium/core that nothing imported — so the
      // bare specifier now means the wasm build and nothing else, which is why
      // it is aliased again here: since 0.2.0 the controller loads its core
      // with `import("iridium-bindings")` rather than by walking a relative
      // path out of the package, so this alias is what the example resolves.
      "iridium-bindings/wasm": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
      "iridium-bindings": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
      // Resolve web-tree-sitter for files outside this directory
      "web-tree-sitter": resolve(__dirname, "node_modules/web-tree-sitter"),
    },
  },
  optimizeDeps: {
    exclude: ["@iridium/core", "@iridium/syntax-worker", "iridium-bindings", "iridium-bindings/wasm"],
  },
  build: {
    target: "esnext",
  },
  server: {
    port: 12224,
    fs: {
      // Allow serving files from project root
      allow: ["../.."],
    },
  },
});
