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
    outDir: resolve(root, "crates/preview/src/embedded/generated"),
    lib: {
      entry: resolve(root, "frontend/boot/main.ts"),
      name: "GtlThemeBoot",
      formats: ["iife"],
      fileName: () => "boot.js",
    },
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
  },
});
