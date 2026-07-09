// Bun-only entrypoint: seeds the manifest's live-view fixtures into `<GIT_TOOLS_DATA_DIR>/gtl.db`
// AFTER `fixtures.ts` has built them. Kept separate from `fixtures.ts` on purpose — it imports
// `bun:sqlite` (via `db.ts`), and the WDIO spec files statically import `fixtures.ts` under
// Node's ESM loader, which rejects the `bun:` URL scheme. Isolating the SQLite import here keeps
// it out of the spec's module graph. Run right after `fixtures` in the package `fixtures` script.
import { loadFixtureManifest } from "./fixtures";
import { seedLiveViews } from "./db";

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}

function main(): void {
  const manifest = loadFixtureManifest();
  const dataDir = requiredEnv("GIT_TOOLS_DATA_DIR");
  seedLiveViews(dataDir, [
    {
      sourceKind: manifest.liveViews.healthy.sourceKind,
      sourceValue: manifest.liveViews.healthy.sourceValue,
      displayName: manifest.liveViews.healthy.displayName,
    },
    {
      sourceKind: manifest.liveViews.broken.sourceKind,
      sourceValue: manifest.liveViews.broken.sourceValue,
      displayName: manifest.liveViews.broken.displayName,
    },
  ]);
}

if (import.meta.main) {
  main();
}
