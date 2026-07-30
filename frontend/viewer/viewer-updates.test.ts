import { describe, expect, test } from "vitest";
import { createViewerUpdateCoordinator } from "./viewer-updates";

type ViewerUpdate = Parameters<ReturnType<typeof createViewerUpdateCoordinator>>[0];

function deferredUpdates() {
  const started: ViewerUpdate[] = [];
  const releases: Array<() => void> = [];
  let activeUpdates = 0;
  let activeUpdatesMax = 0;

  return {
    started,
    activeUpdatesMax: () => activeUpdatesMax,
    run: (update: ViewerUpdate) => {
      started.push(update);
      activeUpdates += 1;
      activeUpdatesMax = Math.max(activeUpdatesMax, activeUpdates);
      return new Promise<void>((resolve) => {
        releases.push(() => {
          activeUpdates -= 1;
          resolve();
        });
      });
    },
    releaseNext: async () => {
      const release = releases.shift();
      if (!release) throw new Error("no viewer update is waiting");
      release();
      await Promise.resolve();
    },
  };
}

describe("createViewerUpdateCoordinator", () => {
  test("serializes update kinds, prioritizes pending recipes, and bounds duplicate work", async () => {
    const updates = deferredUpdates();
    const requestUpdate = createViewerUpdateCoordinator(updates.run, () => {});

    const settled = requestUpdate("ready-view");
    void requestUpdate("ready-view");
    void requestUpdate("pending-recipes");
    void requestUpdate("pending-recipes");

    expect(updates.started).toEqual(["ready-view"]);

    await updates.releaseNext();
    expect(updates.started).toEqual(["ready-view", "pending-recipes"]);

    await updates.releaseNext();
    expect(updates.started).toEqual(["ready-view", "pending-recipes", "ready-view"]);

    await updates.releaseNext();
    await settled;

    expect(updates.activeUpdatesMax()).toBe(1);
  });

  test("reports a failed update and remains usable", async () => {
    const failures: Array<{ update: ViewerUpdate; error: unknown }> = [];
    const completed: ViewerUpdate[] = [];
    let failNext = true;
    const requestUpdate = createViewerUpdateCoordinator(
      (update) => {
        if (failNext) {
          failNext = false;
          return Promise.reject(new Error("unavailable"));
        }
        completed.push(update);
        return Promise.resolve();
      },
      (update, error) => failures.push({ update, error }),
    );

    await requestUpdate("ready-view");
    await requestUpdate("pending-recipes");

    expect(failures.map(({ update }) => update)).toEqual(["ready-view"]);
    expect(completed).toEqual(["pending-recipes"]);
  });
});
