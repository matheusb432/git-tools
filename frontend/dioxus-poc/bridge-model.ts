export type BridgeUpdate = "pending-recipes" | "ready-fragment";

export type DiffFragmentState =
  | { readonly state: "pending" }
  | {
      readonly state: "ready";
      readonly fragment: { readonly html: string; readonly css: string };
    };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function parseDiffFragmentState(value: unknown): DiffFragmentState | null {
  if (!isRecord(value) || typeof value.state !== "string") return null;
  if (value.state === "pending") return { state: "pending" };
  if (value.state !== "ready" || !isRecord(value.fragment)) return null;
  const { html, css } = value.fragment;
  return typeof html === "string" && typeof css === "string" ? { state: "ready", fragment: { html, css } } : null;
}

export function createUpdateCoordinator(
  runUpdate: (update: BridgeUpdate) => Promise<void>,
  reportFailure: (update: BridgeUpdate, error: unknown) => void = () => {},
): (update: BridgeUpdate) => Promise<void> {
  const requested = new Set<BridgeUpdate>();
  let running: Promise<void> | null = null;

  const drain = async (): Promise<void> => {
    while (requested.size > 0) {
      const update = requested.has("pending-recipes") ? "pending-recipes" : "ready-fragment";
      requested.delete(update);
      try {
        await runUpdate(update);
      } catch (error) {
        reportFailure(update, error);
      }
    }
    running = null;
  };

  return function requestUpdate(update: BridgeUpdate): Promise<void> {
    requested.add(update);
    running ??= drain();
    return running;
  };
}
