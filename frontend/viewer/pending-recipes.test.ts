import { afterEach, describe, expect, test } from "vitest";
import { createCoalescedDrain, installPendingRecipes } from "./pending-recipes";

afterEach(() => {
  document.body.innerHTML = "";
});

function mountReadyView(): HTMLElement {
  const view = document.createElement("section");
  view.id = "viewer-view";
  view.dataset["viewerState"] = "ready";
  const template = document.createElement("template");
  template.id = "viewer-loading-template";
  const skeleton = document.createElement("section");
  skeleton.id = "viewer-view";
  skeleton.setAttribute("data-viewer-state", "loading");
  Object.defineProperty(template, "content", {
    value: { firstElementChild: skeleton },
  });
  document.body.appendChild(view);
  document.body.appendChild(template);
  return view;
}

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

  test("preserves the ready view during the closing drain and loads after an event", async () => {
    const readyView = mountReadyView();
    let eventHandler = (): void => {};
    let refreshes = 0;
    await installPendingRecipes({
      document,
      __TAURI__: {
        event: {
          listen: (_event: string, handler: () => void) => {
            eventHandler = handler;
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

    expect(document.getElementById("viewer-view")).toBe(readyView);

    eventHandler();

    expect(document.getElementById("viewer-view")).toBe(readyView);
    expect(document.getElementById("viewer-view")?.dataset["viewerState"]).toBe("loading");
    expect(refreshes).toBe(2);
  });
});
