import { describe, expect, test } from "bun:test";

import { buildEnvironmentEvidence } from "./evidence";

describe("buildEnvironmentEvidence", () => {
  test("records source revision and machine metadata", () => {
    const evidence = buildEnvironmentEvidence({
      capabilities: { browserName: "wry" },
      userAgent: "test-agent",
      screenshots: ["final.png"],
      env: {
        GTL_NATIVE_PERF_GIT_COMMIT: "abc123",
        GTL_NATIVE_PERF_GIT_BRANCH: "main",
        GTL_NATIVE_PERF_GIT_DIRTY: "true",
        GTL_NATIVE_PERF_SMOKE: "1",
        GTL_NATIVE_PERF_APP_BINARY: "/repo/target/release/gtl-viewer",
        GTL_NATIVE_PERF_TAURI_DRIVER: "/home/me/.cargo/bin/tauri-driver",
        GTL_NATIVE_PERF_FIXTURE_ROOT: "/tmp/fixtures",
        GIT_TOOLS_DATA_DIR: "/tmp/data",
      },
    });

    expect(evidence.git).toEqual({
      commit: "abc123",
      branch: "main",
      dirty: true,
    });
    expect(evidence.machine.cpu.logicalCores).toBeGreaterThan(0);
    expect(evidence.machine.memory.totalBytes).toBeGreaterThan(0);
    expect(evidence.os.kernelRelease).toBeTruthy();
  });
});
