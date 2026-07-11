import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { defineConfig } from "vitest/config";

const viewerRoot = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  resolve: { alias: { "@": viewerRoot } },
  test: {
    environment: "happy-dom",
    setupFiles: [resolve(viewerRoot, "vitest.setup.ts")],
    include: [resolve(viewerRoot, "**/*.component.test.ts"), resolve(viewerRoot, "**/*.query.test.ts")],
  },
});
