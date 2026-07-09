import { expect, test } from "bun:test";
import { batchMessage, createToast, DEFAULT_TOAST_TIMEOUT_MS, dismissFrom, type ToastItem } from "./toast";

test("createToast builds an item with the given id, message, and default timeout", () => {
  const toast = createToast(7, "Opened 3 diffs — gtl diff --all");
  expect(toast).toEqual({
    id: 7,
    message: "Opened 3 diffs — gtl diff --all",
    timeoutMs: DEFAULT_TOAST_TIMEOUT_MS,
  });
});

test("createToast accepts a custom timeout", () => {
  const toast = createToast(1, "hi", 1000);
  expect(toast.timeoutMs).toBe(1000);
});

test("dismissFrom removes the matching item and leaves the rest", () => {
  const list: ToastItem[] = [createToast(1, "first"), createToast(2, "second")];
  expect(dismissFrom(list, 1)).toEqual([createToast(2, "second")]);
});

test("dismissFrom on an unknown id is a no-op and returns a new array", () => {
  const list: ToastItem[] = [createToast(1, "only one")];
  const result = dismissFrom(list, 999);
  expect(result).toEqual(list);
  expect(result).not.toBe(list);
});

test("batchMessage returns null for a single diff", () => {
  expect(batchMessage(1, "gtl diff")).toBeNull();
});

test("batchMessage returns null for zero or negative counts", () => {
  expect(batchMessage(0, "gtl diff --all")).toBeNull();
  expect(batchMessage(-1, "gtl diff --all")).toBeNull();
});

test("batchMessage returns announcement copy for more than one diff", () => {
  expect(batchMessage(2, "gtl diff --all")).toBe("Opened 2 diffs — gtl diff --all");
});
