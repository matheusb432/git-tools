import { expect, test, vi } from "vitest";
import { createEnhancementLifecycle } from "./enhancement-lifecycle";

test("mounts and tears down each root exactly once", () => {
  const scope = document.createElement("section");
  const first = document.createElement("div");
  const second = document.createElement("div");
  first.className = "layout";
  second.className = "layout";
  scope.appendChild(first);
  scope.appendChild(second);

  const unmountFirst = vi.fn();
  const unmountSecond = vi.fn();
  const mount = vi.fn((root: HTMLElement) => (root === first ? unmountFirst : unmountSecond));
  const lifecycle = createEnhancementLifecycle(".layout", mount);

  lifecycle.enhanceWithin(scope);
  lifecycle.enhanceWithin(scope);

  expect(mount).toHaveBeenCalledTimes(2);
  expect(first.dataset["gtlEnhanced"]).toBe("true");
  expect(second.dataset["gtlEnhanced"]).toBe("true");

  lifecycle.destroyWithin(scope);
  lifecycle.destroyWithin(scope);

  expect(unmountFirst).toHaveBeenCalledOnce();
  expect(unmountSecond).toHaveBeenCalledOnce();
  expect(first.dataset["gtlEnhanced"]).toBeUndefined();
  expect(second.dataset["gtlEnhanced"]).toBeUndefined();
});

test("accepts the root itself as the enhancement scope", () => {
  const root = document.createElement("div");
  root.className = "layout";
  const mount = vi.fn((_root: HTMLElement) => vi.fn());
  const lifecycle = createEnhancementLifecycle(".layout", mount);

  lifecycle.enhanceWithin(root);

  expect(mount.mock.calls[0]?.[0]).toBe(root);
});
