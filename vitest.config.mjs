import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  test: {
    setupFiles: ["./frontend/test/dom-stub.ts"],
  },
});
