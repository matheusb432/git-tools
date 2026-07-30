import { showViewerLoading } from "./view-loading";

const PENDING_RECIPES_EVENT = "recipes-pending";
const RECIPE_COMPLETED_EVENT = "recipe-completed";

type ViewerUpdate = "pending-recipes" | "ready-view";

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

type ViewerRuntime = {
  readonly document: Document;
  readonly events: TauriEvents;
  readonly htmx: HtmxAjax;
};

function listensToEvents(value: unknown): value is TauriEvents {
  return typeof value === "object" && value !== null && "listen" in value && typeof value.listen === "function";
}

function sendsAjax(value: unknown): value is HtmxAjax {
  return typeof value === "object" && value !== null && "ajax" in value && typeof value.ajax === "function";
}

function readViewerRuntime(host: unknown): ViewerRuntime | null {
  if (typeof host !== "object" || host === null || !("document" in host)) return null;
  if (!(host.document instanceof Document) || !("__TAURI__" in host) || !("htmx" in host)) return null;
  const tauri = host.__TAURI__;
  if (typeof tauri !== "object" || tauri === null || !("event" in tauri)) return null;
  return listensToEvents(tauri.event) && sendsAjax(host.htmx)
    ? { document: host.document, events: tauri.event, htmx: host.htmx }
    : null;
}

function runViewerUpdate(htmx: HtmxAjax, update: ViewerUpdate): Promise<void> {
  switch (update) {
    case "pending-recipes":
      return htmx.ajax("GET", "/pending", {
        target: "#viewer-tabs",
        swap: "outerHTML",
      });
    case "ready-view":
      return htmx.ajax("GET", "/ready", {
        target: "#viewer-view",
        swap: "outerHTML",
      });
  }
}

export function createViewerUpdateCoordinator(
  runUpdate: (update: ViewerUpdate) => Promise<void>,
  reportFailure: (update: ViewerUpdate, error: unknown) => void,
): (update: ViewerUpdate) => Promise<void> {
  const requestedUpdates = new Set<ViewerUpdate>();
  let running: Promise<void> | null = null;

  const drain = async (): Promise<void> => {
    while (requestedUpdates.size > 0) {
      const update = requestedUpdates.has("pending-recipes") ? "pending-recipes" : "ready-view";
      requestedUpdates.delete(update);
      try {
        await runUpdate(update);
      } catch (error) {
        reportFailure(update, error);
      }
    }
    running = null;
  };

  return function requestUpdate(update: ViewerUpdate): Promise<void> {
    requestedUpdates.add(update);
    running ??= drain();
    return running;
  };
}

export async function installViewerUpdates(host: unknown): Promise<void> {
  const runtime = readViewerRuntime(host);
  if (!runtime) return;

  const requestUpdate = createViewerUpdateCoordinator(
    (update) => runViewerUpdate(runtime.htmx, update),
    (update, error) => console.error(`failed to apply viewer update ${update}`, error),
  );
  const pendingSubscription = runtime.events
    .listen(PENDING_RECIPES_EVENT, () => {
      showViewerLoading(runtime.document);
      void requestUpdate("pending-recipes");
    })
    .catch((error: unknown) => console.error("failed to subscribe to pending recipes", error));
  const readySubscription = runtime.events
    .listen(RECIPE_COMPLETED_EVENT, () => void requestUpdate("ready-view"))
    .catch((error: unknown) => console.error("failed to subscribe to recipe completion", error));

  await Promise.all([pendingSubscription, readySubscription]);
  await requestUpdate("pending-recipes");
  await requestUpdate("ready-view");
}
