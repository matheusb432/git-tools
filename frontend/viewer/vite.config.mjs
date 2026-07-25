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
    outDir: resolve(root, "crates/desktop/src/embedded/generated"),
    lib: {
      entry: resolve(root, "frontend/viewer/main.ts"),
      name: "GtlViewer",
      formats: ["iife"],
      fileName: () => "viewer.js",
    },
  },
});
