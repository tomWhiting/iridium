import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  resolve: {
    alias: {
      "iridium-bindings/element": resolve(__dirname, "../../crates/iridium-bindings/ts/element/index.ts"),
      "iridium-bindings": resolve(__dirname, "../../crates/iridium-bindings/ts/controller/index.ts"),
      // Resolve web-tree-sitter for files outside this directory
      "web-tree-sitter": resolve(__dirname, "node_modules/web-tree-sitter"),
    },
  },
  optimizeDeps: {
    exclude: ["iridium-bindings"],
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
