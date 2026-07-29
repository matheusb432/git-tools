import { afterEach, describe, expect, test, vi } from "vitest";
import { createCoalescedReadyRefresh, installViewerLoading, showViewerLoading } from "./view-loading";

afterEach(() => {
  document.body.innerHTML = "";
});

function mountLoadingTemplate(): HTMLElement {
  const view = document.createElement("section");
  view.id = "viewer-view";
  view.dataset["viewerState"] = "ready";
  const template = document.createElement("template");
  template.id = "viewer-loading-template";
  const skeleton = document.createElement("section");
  skeleton.id = "viewer-view";
  skeleton.setAttribute("data-viewer-state", "loading");
  skeleton.setAttribute("aria-busy", "true");
  const status = document.createElement("div");
  status.className = "loading-marker";
  skeleton.appendChild(status);
  Object.defineProperty(template, "content", {
    value: { firstElementChild: skeleton },
  });
  document.body.appendChild(view);
  document.body.appendChild(template);
  return view;
}

describe("showViewerLoading", () => {
  test("replaces the active view from the static template", () => {
    const previous = mountLoadingTemplate();

    expect(showViewerLoading(document)).toBe(true);
    const replacement = document.getElementById("viewer-view");
    expect(replacement).toBe(previous);
    expect(replacement?.dataset["viewerState"]).toBe("loading");
    expect(replacement?.querySelector(".loading-marker")).not.toBeNull();
  });
});

describe("installViewerLoading", () => {
  test("subscribes before the closing readiness request", async () => {
    mountLoadingTemplate();
    const ajax = vi.fn(() => Promise.resolve());
    let subscribed = false;

    await installViewerLoading({
      document,
      __TAURI__: {
        event: {
          listen: () => {
            subscribed = true;
            expect(ajax).not.toHaveBeenCalled();
            return Promise.resolve(() => {});
          },
        },
      },
      htmx: { ajax },
    });

    expect(subscribed).toBe(true);
    expect(ajax).toHaveBeenCalledWith("GET", "/ready", {
      target: "#viewer-view",
      swap: "outerHTML",
    });
  });

  test("does not wait for event subscription before the readiness request", async () => {
    mountLoadingTemplate();
    const ajax = vi.fn(() => Promise.resolve());
    let releaseSubscription = (): void => {};
    const installed = installViewerLoading({
      document,
      __TAURI__: {
        event: {
          listen: () =>
            new Promise<void>((resolve) => {
              releaseSubscription = resolve;
            }),
        },
      },
      htmx: { ajax },
    });

    await Promise.resolve();
    expect(ajax).toHaveBeenCalledOnce();

    releaseSubscription();
    await installed;
  });

  test("shows loading before htmx handles a view request", async () => {
    const previous = mountLoadingTemplate();
    const trigger = document.createElement("button");
    trigger.setAttribute("hx-get", "/tabs/2/activate");
    trigger.setAttribute("hx-target", "#viewer-view");
    document.body.appendChild(trigger);
    await installViewerLoading({ document });

    trigger.click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(document.getElementById("viewer-view")).toBe(previous);
    expect(document.getElementById("viewer-view")?.dataset["viewerState"]).toBe("loading");
  });

  test("discards a loader scheduled for a view htmx already replaced", async () => {
    const previous = mountLoadingTemplate();
    const trigger = document.createElement("button");
    trigger.setAttribute("hx-get", "/tabs/2/activate");
    trigger.setAttribute("hx-target", "#viewer-view");
    document.body.appendChild(trigger);
    await installViewerLoading({ document });

    trigger.click();
    const completed = document.createElement("section");
    completed.id = "viewer-view";
    completed.dataset["viewerState"] = "ready";
    previous.replaceWith(completed);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(document.getElementById("viewer-view")).toBe(completed);
    expect(completed.dataset["viewerState"]).toBe("ready");
  });
});

describe("createCoalescedReadyRefresh", () => {
  test("bounds a completion burst to one running request and one rerun", async () => {
    let refreshes = 0;
    let release = (): void => {};
    const refresh = createCoalescedReadyRefresh(
      () => {
        refreshes += 1;
        if (refreshes > 1) return Promise.resolve();
        return new Promise<void>((resolve) => {
          release = resolve;
        });
      },
      () => {},
    );

    const settled = refresh();
    refresh();
    refresh();
    expect(refreshes).toBe(1);

    release();
    await settled;

    expect(refreshes).toBe(2);
  });
});
