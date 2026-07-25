import { describe, expect, test } from "vitest";
import { createCoalescedDrain, installPendingRecipes } from "./pending-recipes";

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
  test("subscribes before the closing drain runs", async () => {
    let refreshes = 0;
    await installPendingRecipes({
      __TAURI__: {
        event: {
          listen: () => {
            expect(refreshes).toBe(0);
            return Promise.resolve(() => {});
          },
        },
      },
      htmx: {
        ajax: () => {
          refreshes += 1;
          return Promise.resolve();
        },
      },
    });

    expect(refreshes).toBe(1);
  });
});
