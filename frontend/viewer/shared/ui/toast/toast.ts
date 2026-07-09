/** A single transient notification rendered by {@link Toast}. */
export type ToastItem = {
  readonly id: number;
  readonly message: string;
  readonly timeoutMs: number;
};

export const DEFAULT_TOAST_TIMEOUT_MS = 4000;

/** Builds a toast item; the caller owns id allocation and list mutation. */
export function createToast(id: number, message: string, timeoutMs = DEFAULT_TOAST_TIMEOUT_MS): ToastItem {
  return { id, message, timeoutMs };
}

/** Returns a new list with the toast matching `id` removed; a no-op if absent. */
export function dismissFrom(list: readonly ToastItem[], id: number): ToastItem[] {
  return list.filter((toast) => toast.id !== id);
}

/** Batch-open announcement copy, or null when there's nothing worth announcing (count <= 1). */
export function batchMessage(count: number, commandLabel: string): string | null {
  return count > 1 ? `Opened ${count} diffs — ${commandLabel}` : null;
}
