import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    exclude: [...configDefaults.exclude, ".artifacts/**"],
    setupFiles: ["./frontend/test/dom-stub.ts"],
  },
});
