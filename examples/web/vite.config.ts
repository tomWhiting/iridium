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
      // New @iridium/core package (recommended)
      "@iridium/core": resolve(__dirname, "../../packages/@iridium/core/src/controller/index.ts"),
      "@iridium/core/syntax": resolve(__dirname, "../../packages/@iridium/core/src/syntax/index.ts"),
      "@iridium/core/element": resolve(__dirname, "../../packages/@iridium/core/src/element/index.ts"),
      // Legacy iridium-bindings (still supported)
      "iridium-bindings": resolve(__dirname, "../../crates/iridium-bindings/ts/controller/index.ts"),
      "iridium-bindings/wasm": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
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
