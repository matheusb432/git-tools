import { expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, relative } from "node:path";

const entitiesRoot = join(import.meta.dir);
const sameLayerEntityImport = /from\s+["']@\/entities\/([^/"']+)/g;

function sourceFiles(root: string): readonly string[] {
  const entries = readdirSync(root).flatMap((entry) => {
    const path = join(root, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) return sourceFiles(path);
    return path.endsWith(".ts") || path.endsWith(".svelte") ? [path] : [];
  });
  return entries;
}

test("entity slices do not import sibling entity slices", () => {
  const violations: string[] = [];

  for (const file of sourceFiles(entitiesRoot)) {
    const currentSlice = relative(entitiesRoot, file).split("/")[0];
    const source = readFileSync(file, "utf8");

    for (const match of source.matchAll(sameLayerEntityImport)) {
      const importedSlice = match[1];
      if (importedSlice !== undefined && importedSlice !== currentSlice) {
        violations.push(`${basename(file)} imports @/entities/${importedSlice}`);
      }
    }
  }

  expect(violations).toEqual([]);
});
