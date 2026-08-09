import { createDiffIslandAdapter, type DiffIslandAdapter } from "./adapter";

declare global {
  interface Window {
    GtlDiffIsland?: DiffIslandAdapter;
  }
}

window.GtlDiffIsland?.destroy();
window.GtlDiffIsland = createDiffIslandAdapter();
