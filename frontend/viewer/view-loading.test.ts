import { afterEach, describe, expect, test, vi } from "vitest";
import { installViewerLoading, showViewerLoading } from "./view-loading";

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
  skeleton.textContent = "Loading diff";
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
    expect(replacement).not.toBe(previous);
    expect(replacement?.dataset["viewerState"]).toBe("loading");
    expect(replacement?.textContent).toContain("Loading diff");
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

  test("shows loading before htmx handles a view request", async () => {
    const previous = mountLoadingTemplate();
    const trigger = document.createElement("button");
    trigger.setAttribute("hx-get", "/tabs/2/activate");
    trigger.setAttribute("hx-target", "#viewer-view");
    document.body.appendChild(trigger);
    await installViewerLoading({ document });

    trigger.click();

    expect(document.getElementById("viewer-view")).not.toBe(previous);
    expect(document.getElementById("viewer-view")?.dataset["viewerState"]).toBe("loading");
  });
});
