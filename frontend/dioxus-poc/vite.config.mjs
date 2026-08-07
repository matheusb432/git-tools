import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

export default defineConfig({
  root,
  build: {
    target: "es2021",
    minify: true,
    emptyOutDir: false,
    outDir: resolve(root, "crates/gtl-web/assets/generated"),
    lib: {
      entry: resolve(root, "frontend/dioxus-poc/bridge.ts"),
      name: "GtlDioxusProof",
      formats: ["iife"],
      fileName: () => "dioxus-poc.js",
    },
  },
});
