import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const viewerRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(viewerRoot, "../..");

export default defineConfig({
  root: viewerRoot,
  base: "./",
  plugins: [tailwindcss(), svelte()],
  resolve: { alias: { "@": viewerRoot } },
  build: {
    target: "es2021",
    outDir: resolve(repoRoot, "crates/desktop/dist"),
    emptyOutDir: true,
  },
});
