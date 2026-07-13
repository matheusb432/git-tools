import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    setupFiles: ["./frontend/test/dom-stub.ts"],
  },
});
