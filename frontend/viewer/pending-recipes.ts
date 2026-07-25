const PENDING_RECIPES_EVENT = "recipes-pending";
const PENDING_RECIPES_ROUTE = "/pending";
const PENDING_TABS_TARGET = "#viewer-tabs";
const PENDING_TABS_SWAP = "outerHTML";

type ViewerRuntime = Pick<Window, "__TAURI__" | "htmx">;

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
export async function installPendingRecipes(runtime: ViewerRuntime): Promise<void> {
  const tauri = runtime.__TAURI__;
  const htmx = runtime.htmx;
  if (!tauri || !htmx) return;
  const drain = createCoalescedDrain(
    () => htmx.ajax("GET", PENDING_RECIPES_ROUTE, { target: PENDING_TABS_TARGET, swap: PENDING_TABS_SWAP }),
    (error) => console.error("failed to drain pending recipes", error),
  );
  try {
    await tauri.event.listen(PENDING_RECIPES_EVENT, () => void drain());
  } catch (error) {
    console.error("failed to subscribe to pending recipes", error);
    return;
  }
  await drain();
}
