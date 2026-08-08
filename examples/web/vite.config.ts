import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // New @iridium/core package (recommended).
      //
      // Longest prefix first: a string alias matches `find` *or* anything under
      // `find/`, and the first match wins — so a bare "@iridium/core" listed
      // ahead of these would swallow every subpath and rewrite it to a path
      // underneath controller/index.ts.
      "@iridium/core/syntax": resolve(__dirname, "../../packages/@iridium/core/src/syntax/index.ts"),
      "@iridium/core/element": resolve(__dirname, "../../packages/@iridium/core/src/element/index.ts"),
      "@iridium/core/palette": resolve(__dirname, "../../packages/@iridium/core/src/palette/index.ts"),
      "@iridium/core/history": resolve(__dirname, "../../packages/@iridium/core/src/history/index.ts"),
      "@iridium/core": resolve(__dirname, "../../packages/@iridium/core/src/controller/index.ts"),
      // The wasm bundle. The `ts/` half of iridium-bindings was deleted with
      // #64 — a January fork of @iridium/core that nothing imported — so the
      // bare specifier now means the wasm build and nothing else, which is why
      // it is aliased again here: since 0.2.0 the controller loads its core
      // with `import("iridium-bindings")` rather than by walking a relative
      // path out of the package, so this alias is what the example resolves.
      "iridium-bindings/wasm": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
      "iridium-bindings": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
      "iridium-wasm": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
      // Resolve web-tree-sitter for files outside this directory
      "web-tree-sitter": resolve(__dirname, "node_modules/web-tree-sitter"),
    },
  },
  optimizeDeps: {
    exclude: ["@iridium/core", "iridium-bindings", "iridium-bindings/wasm", "iridium-wasm"],
  },
  build: {
    target: "esnext",
  },
  server: {
    port: 12223,
    fs: {
      // Allow serving files from project root (two levels up from examples/web)
      allow: ["../.."],
    },
  },
  preview: {
    port: 12223,
  },
});
