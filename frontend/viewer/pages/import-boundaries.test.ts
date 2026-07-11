import { expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

const viewerRoot = join(import.meta.dir, "..");

function sourceFiles(root: string): readonly string[] {
  if (!existsSync(root)) return [];
  return readdirSync(root).flatMap((entry) => {
    const path = join(root, entry);
    return statSync(path).isDirectory() ? sourceFiles(path) : /\.(?:ts|svelte)$/.test(path) ? [path] : [];
  });
}

function viewerPath(file: string): string {
  return relative(viewerRoot, file).split(sep).join("/");
}

test("viewer uses pages-first FSD without legacy single-use layers", () => {
  expect(existsSync(join(viewerRoot, "widgets"))).toBe(false);
  expect(existsSync(join(viewerRoot, "entities"))).toBe(false);
  expect(existsSync(join(viewerRoot, "pages", "viewer", "index.ts"))).toBe(true);
  expect(existsSync(join(viewerRoot, "pages", "history", ".gitkeep"))).toBe(true);
  expect(readdirSync(join(viewerRoot, "pages", "history"))).toEqual([".gitkeep"]);
});

test("page slices never import sibling pages or bypass the viewer public API", () => {
  const violations: string[] = [];
  for (const file of sourceFiles(join(viewerRoot, "pages"))) {
    const currentPage = relative(join(viewerRoot, "pages"), file).split(sep)[0];
    for (const match of readFileSync(file, "utf8").matchAll(/from\s+["']@\/pages\/([^/"']+)([^"']*)["']/g)) {
      const importedPage = match[1];
      if (importedPage !== undefined && importedPage !== currentPage) {
        violations.push(`${viewerPath(file)} imports sibling page ${importedPage}`);
      }
    }
  }
  const appSource = sourceFiles(join(viewerRoot, "app"))
    .map((file) => readFileSync(file, "utf8"))
    .join("\n");
  if (/from\s+["']@\/pages\/viewer\//.test(appSource)) violations.push("app bypasses @/pages/viewer public API");
  expect(violations).toEqual([]);
});

test("transport and virtualizer imports stay at their infrastructure boundaries", () => {
  const violations: string[] = [];
  for (const file of sourceFiles(viewerRoot)) {
    const path = viewerPath(file);
    if (path.endsWith(".test.ts")) continue;
    const source = readFileSync(file, "utf8");
    if (source.includes("__TAURI__") && path !== "shared/api/tauri.ts") violations.push(`${path} touches __TAURI__`);
    if (source.includes("@tanstack/svelte-virtual") && !path.endsWith("ui/diff-view/VirtualFileList.svelte")) {
      violations.push(`${path} imports @tanstack/svelte-virtual`);
    }
  }
  expect(violations).toEqual([]);
});
