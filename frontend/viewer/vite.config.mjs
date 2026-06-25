import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

export default defineConfig({
  root,
  plugins: [svelte({ emitCss: false })],
  build: {
    target: "es2021",
    minify: true,
    emptyOutDir: false,
    outDir: resolve(root, "crates/desktop/dist"),
    lib: {
      entry: resolve(root, "frontend/viewer/main.ts"),
      name: "GtlViewerShell",
      formats: ["iife"],
      fileName: () => "shell.js",
    },
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
  },
});
