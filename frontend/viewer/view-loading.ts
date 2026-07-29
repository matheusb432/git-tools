const LOADING_TEMPLATE_ID = "viewer-loading-template";
const RECIPE_COMPLETED_EVENT = "recipe-completed";
const READY_ROUTE = "/ready";

type TauriEvents = {
  readonly listen: (event: string, handler: () => void) => Promise<unknown>;
};

type HtmxAjax = {
  readonly ajax: (
    verb: string,
    path: string,
    context: { readonly target: string; readonly swap: string },
  ) => Promise<void>;
};

type ViewerHost = {
  readonly document: Document;
  readonly __TAURI__?: { readonly event?: TauriEvents };
  readonly htmx?: HtmxAjax;
};

function viewerHost(value: unknown): ViewerHost | null {
  if (typeof value !== "object" || value === null || !("document" in value)) return null;
  return value as ViewerHost;
}

export function showViewerLoading(targetDocument: Document): boolean {
  const template = targetDocument.getElementById(LOADING_TEMPLATE_ID);
  const current = targetDocument.getElementById("viewer-view");
  if (!(template instanceof HTMLTemplateElement) || !(current instanceof HTMLElement)) return false;
  const replacement = template.content.firstElementChild?.cloneNode(true);
  if (!(replacement instanceof HTMLElement)) return false;
  current.replaceWith(replacement);
  targetDocument.querySelector(".viewer-tab.active")?.setAttribute("aria-busy", "true");
  return true;
}

function targetsViewerView(element: Element): boolean {
  return element.getAttribute("hx-target") === "#viewer-view";
}

function opensHistoryView(element: Element): boolean {
  const route = element.getAttribute("hx-get");
  return route !== null && route.startsWith("/history/") && route.endsWith("/open");
}

function activeTabClose(element: Element): boolean {
  return (
    element.classList.contains("viewer-tab-close") &&
    element.closest(".viewer-tab")?.classList.contains("active") === true
  );
}

export async function installViewerLoading(hostValue: unknown): Promise<void> {
  const host = viewerHost(hostValue);
  if (!host) return;
  const { document: targetDocument } = host;
  if (targetDocument.getElementById("viewer-view")?.getAttribute("data-viewer-state") === "loading") {
    showViewerLoading(targetDocument);
  }
  targetDocument.addEventListener(
    "click",
    (event) => {
      const trigger = event.target instanceof Element ? event.target.closest("[hx-get]") : null;
      if (!trigger) return;
      if (targetsViewerView(trigger) || opensHistoryView(trigger) || activeTabClose(trigger)) {
        showViewerLoading(targetDocument);
      }
    },
    { capture: true },
  );

  const events = host.__TAURI__?.event;
  const htmx = host.htmx;
  if (!events || typeof events.listen !== "function" || !htmx || typeof htmx.ajax !== "function") return;
  const refresh = (): Promise<void> => htmx.ajax("GET", READY_ROUTE, { target: "#viewer-view", swap: "outerHTML" });
  try {
    await events.listen(RECIPE_COMPLETED_EVENT, () => void refresh());
  } catch (error) {
    console.error("failed to subscribe to recipe completion", error);
    return;
  }
  await refresh();
}
