const PENDING_RECIPES_EVENT = "recipes-pending";
const PENDING_RECIPES_ROUTE = "/pending";
const PENDING_TABS_TARGET = "#viewer-tabs";
const PENDING_TABS_SWAP = "outerHTML";

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
  readonly events: TauriEvents;
  readonly htmx: HtmxAjax;
};

function listensToEvents(value: unknown): value is TauriEvents {
  return typeof value === "object" && value !== null && "listen" in value && typeof value.listen === "function";
}

function sendsAjax(value: unknown): value is HtmxAjax {
  return typeof value === "object" && value !== null && "ajax" in value && typeof value.ajax === "function";
}

// Tauri injects __TAURI__ and the inlined htmx bootstrap assigns htmx, both onto the host object
// before this bundle runs. Neither exists in the artifact, so the shape is proven here rather than
// declared on Window, which would make these host globals visible to every bundle.
function readViewerRuntime(host: unknown): ViewerRuntime | null {
  if (typeof host !== "object" || host === null) return null;
  if (!("__TAURI__" in host) || !("htmx" in host)) return null;
  const tauri = host.__TAURI__;
  if (typeof tauri !== "object" || tauri === null || !("event" in tauri)) return null;
  const events = tauri.event;
  const htmx = host.htmx;
  return listensToEvents(events) && sendsAjax(htmx) ? { events, htmx } : null;
}

// At most one refresh runs and at most one rerun is remembered: a refresh started
// after an event already reflects every recipe that event announced, so deeper
// queueing is redundant work. Failures are reported and leave the drain usable.
export function createCoalescedDrain(
  refresh: () => Promise<void>,
  reportFailure: (error: unknown) => void,
): () => Promise<void> {
  let running: Promise<void> | null = null;
  let rerunRequested = false;
  return function drain(): Promise<void> {
    if (running) {
      rerunRequested = true;
      return running;
    }
    running = (async () => {
      do {
        rerunRequested = false;
        try {
          await refresh();
        } catch (error) {
          reportFailure(error);
        }
      } while (rerunRequested);
      running = null;
    })();
    return running;
  };
}

// Subscribes before the first drain so a recipe published in between is never
// missed; one closing drain then covers everything queued before the subscription.
export async function installPendingRecipes(host: unknown): Promise<void> {
  const runtime = readViewerRuntime(host);
  if (!runtime) return;
  const drain = createCoalescedDrain(
    () => runtime.htmx.ajax("GET", PENDING_RECIPES_ROUTE, { target: PENDING_TABS_TARGET, swap: PENDING_TABS_SWAP }),
    (error) => console.error("failed to drain pending recipes", error),
  );
  try {
    await runtime.events.listen(PENDING_RECIPES_EVENT, () => void drain());
  } catch (error) {
    console.error("failed to subscribe to pending recipes", error);
    return;
  }
  await drain();
}
