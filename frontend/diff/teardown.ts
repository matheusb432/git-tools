type Timer = ReturnType<typeof setTimeout>;

/** Everything one mount has to undo: listeners in reverse order, then any timer still pending. */
export type Teardown = {
  readonly listen: (
    target: EventTarget,
    type: string,
    listener: EventListener,
    options?: AddEventListenerOptions,
  ) => void;
  readonly later: (callback: () => void, delay: number) => Timer;
  readonly cancel: (timer: Timer) => void;
  readonly destroy: () => void;
};

export function createTeardown(): Teardown {
  const cleanups: Array<() => void> = [];
  const timers = new Set<Timer>();

  return {
    listen(target, type, listener, options) {
      target.addEventListener(type, listener, options);
      cleanups.push(() => target.removeEventListener(type, listener, options));
    },
    later(callback, delay) {
      const timer = setTimeout(() => {
        timers.delete(timer);
        callback();
      }, delay);
      timers.add(timer);
      return timer;
    },
    cancel(timer) {
      clearTimeout(timer);
      timers.delete(timer);
    },
    destroy() {
      cleanups.reverse().forEach((cleanup) => cleanup());
      timers.forEach(clearTimeout);
      timers.clear();
    },
  };
}
