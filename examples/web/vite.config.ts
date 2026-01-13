import { defineConfig } from "npm:vite@^6.0.0";
import react from "npm:@vitejs/plugin-react@^4.3.0";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "iridium-bindings": resolve(__dirname, "../../crates/iridium-bindings/ts/controller/index.ts"),
      "iridium-wasm": resolve(__dirname, "../../crates/iridium-bindings/pkg/iridium_bindings.js"),
    },
  },
  optimizeDeps: {
    exclude: ["iridium-bindings", "iridium-wasm"],
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
