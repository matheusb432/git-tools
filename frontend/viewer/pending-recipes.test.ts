import { describe, expect, test } from "vitest";
import {
  createCoalescedDrain,
  installPendingRecipes,
  PENDING_RECIPES_EVENT,
  PENDING_RECIPES_ROUTE,
  PENDING_TABS_SWAP,
  PENDING_TABS_TARGET,
} from "./pending-recipes";

describe("createCoalescedDrain", () => {
  test("a burst during one refresh coalesces into a single rerun", async () => {
    let refreshCount = 0;
    let releaseFirst = (): void => {};
    const drain = createCoalescedDrain(
      () => {
        refreshCount += 1;
        if (refreshCount === 1) {
          return new Promise<void>((resolve) => {
            releaseFirst = resolve;
          });
        }
        return Promise.resolve();
      },
      () => {},
    );

    const settled = drain();
    drain();
    drain();
    drain();
    expect(refreshCount).toBe(1);

    releaseFirst();
    await settled;

    expect(refreshCount).toBe(2);
  });

  test("a failed refresh is reported and the next drain refreshes again", async () => {
    const failures: unknown[] = [];
    let attempts = 0;
    const drain = createCoalescedDrain(
      () => {
        attempts += 1;
        return attempts === 1 ? Promise.reject(new Error("boom")) : Promise.resolve();
      },
      (error) => failures.push(error),
    );

    await drain();
    expect(failures).toHaveLength(1);

    await drain();
    expect(attempts).toBe(2);
  });
});

describe("installPendingRecipes", () => {
  test("subscribes before draining, then drains on every event", async () => {
    const refreshes: Array<{ verb: string; path: string; target: string; swap: string }> = [];
    const published: Array<() => void> = [];
    await installPendingRecipes({
      __TAURI__: {
        event: {
          listen: (event, handler) => {
            expect(event).toBe(PENDING_RECIPES_EVENT);
            expect(refreshes).toHaveLength(0);
            published.push(handler);
            return Promise.resolve(() => {});
          },
        },
      },
      htmx: {
        ajax: (verb, path, context) => {
          refreshes.push({ verb, path, target: context.target, swap: context.swap });
          return Promise.resolve();
        },
      },
    });

    expect(refreshes).toEqual([
      { verb: "GET", path: PENDING_RECIPES_ROUTE, target: PENDING_TABS_TARGET, swap: PENDING_TABS_SWAP },
    ]);

    published[0]?.();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    expect(refreshes).toHaveLength(2);
  });

  test("runs as a no-op outside the tauri host", async () => {
    await expect(installPendingRecipes({})).resolves.toBeUndefined();
  });
});
