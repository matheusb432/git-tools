import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";

const TEST_RESIZE_EVENT = "test:resize-observer";
const resizeObservers = new Set<SynchronousResizeObserver>();

function resizeEntry(target: Element): ResizeObserverEntry {
  const contentRect = target.getBoundingClientRect();
  const size = { inlineSize: contentRect.width, blockSize: contentRect.height };
  return {
    target,
    contentRect,
    borderBoxSize: [size],
    contentBoxSize: [size],
    devicePixelContentBoxSize: [size],
  };
}

class SynchronousResizeObserver implements ResizeObserver {
  readonly #callback: ResizeObserverCallback;
  readonly #listeners = new Map<Element, EventListener>();

  constructor(callback: ResizeObserverCallback) {
    this.#callback = callback;
    resizeObservers.add(this);
  }

  observe(target: Element, _options?: ResizeObserverOptions): void {
    this.unobserve(target);
    const report = () => this.#callback([resizeEntry(target)], this);
    this.#listeners.set(target, report);
    target.addEventListener(TEST_RESIZE_EVENT, report);
    report();
  }

  unobserve(target: Element): void {
    const listener = this.#listeners.get(target);
    if (listener === undefined) return;
    target.removeEventListener(TEST_RESIZE_EVENT, listener);
    this.#listeners.delete(target);
  }

  disconnect(): void {
    for (const target of this.#listeners.keys()) this.unobserve(target);
    resizeObservers.delete(this);
  }
}

globalThis.ResizeObserver = SynchronousResizeObserver;

afterEach(() => {
  for (const observer of resizeObservers) observer.disconnect();
});
