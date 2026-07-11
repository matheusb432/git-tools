export type SerialWakeDrain = {
  readonly wake: () => void;
  readonly dispose: () => void;
};

export function createSerialWakeDrain(drain: () => Promise<void>, onError: (error: Error) => void): SerialWakeDrain {
  let requested = false;
  let running = false;
  let disposed = false;

  const run = async (): Promise<void> => {
    if (running || disposed) return;
    running = true;
    try {
      while (requested && !disposed) {
        requested = false;
        try {
          await drain();
        } catch (error) {
          onError(error instanceof Error ? error : new Error(String(error)));
        }
      }
    } finally {
      running = false;
      if (requested && !disposed) void run();
    }
  };

  return {
    wake: () => {
      if (disposed) return;
      requested = true;
      void run();
    },
    dispose: () => {
      disposed = true;
      requested = false;
    },
  };
}
