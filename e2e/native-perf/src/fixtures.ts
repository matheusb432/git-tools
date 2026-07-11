import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import type { Recipe } from "../../../frontend/viewer/shared/api/index";

const MANIFEST_NAME = "manifest.json";
const LARGE_FILE_LINE_COUNT = 45_000;
const MANY_FILES_COUNT = 300;
const decoder = new TextDecoder();

export type FixtureName = "small" | "two-small-files" | "large-file" | "many-files";

export type FixtureEntry = {
  readonly name: FixtureName;
  readonly repoRoot: string;
  readonly repoName: string;
  readonly primaryFile: string;
  readonly secondaryFile: string | null;
  readonly expectedPrimaryRowText: string;
  readonly expectedSecondaryRowText: string | null;
  readonly recipe: Recipe;
};

/** One saved-source identity for a functional-spec live-view fixture. */
export type LiveViewFixture = {
  readonly sourceKind: "LocalRepo";
  readonly sourceValue: string;
  readonly displayName: string;
};

export type LiveViewFixtures = {
  /** A real temp git repo with unpushed local commits and no remote — probes/computes fine. */
  readonly healthy: LiveViewFixture;
  /** A source path that is never created on disk — probes as `DirNotFound`. */
  readonly broken: LiveViewFixture;
};

export type FixtureManifest = {
  readonly generatedAt: string;
  readonly fixtures: readonly FixtureEntry[];
  readonly liveViews: LiveViewFixtures;
};

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}

function ensureParent(filePath: string): void {
  mkdirSync(dirname(filePath), { recursive: true });
}

function writeRepoFile(repoRoot: string, relativePath: string, contents: string): void {
  const target = join(repoRoot, relativePath);
  ensureParent(target);
  writeFileSync(target, contents, "utf8");
}

function git(repoRoot: string, args: readonly string[]): void {
  const child = Bun.spawnSync({
    cmd: ["git", ...args],
    cwd: repoRoot,
    stdout: "pipe",
    stderr: "pipe",
  });

  if (child.exitCode !== 0) {
    const stderr = decoder.decode(child.stderr).trim();
    const stdout = decoder.decode(child.stdout).trim();
    const detail = stderr !== "" ? stderr : stdout;
    throw new Error(`git ${args.join(" ")} failed in ${repoRoot}: ${detail}`);
  }
}

function initRepo(repoRoot: string): void {
  mkdirSync(repoRoot, { recursive: true });
  git(repoRoot, ["init", "--initial-branch=main"]);
  git(repoRoot, ["config", "user.name", "Native Perf"]);
  git(repoRoot, ["config", "user.email", "native-perf@example.com"]);
}

function commitAll(repoRoot: string, message: string): void {
  git(repoRoot, ["add", "."]);
  git(repoRoot, ["commit", "-m", message]);
}

function diffRecipe(repoRoot: string): Recipe {
  return {
    source: { kind: "LocalRepo", value: repoRoot },
    op: {
      op: "diff",
      target: { target: "unpushed" },
    },
  };
}

function repeatedLines(prefix: string, count: number): string {
  const lines = Array.from({ length: count }, (_, index) => `${prefix} ${String(index + 1).padStart(5, "0")}`);
  return `${lines.join("\n")}\n`;
}

function buildSmallFixture(root: string): FixtureEntry {
  const repoRoot = join(root, "small-repo");
  const primaryFile = "notes.txt";
  const baseLines = Array.from({ length: 120 }, (_, index) => `line ${String(index + 1).padStart(3, "0")}`);
  const updatedLines = [...baseLines];
  updatedLines[59] = "line 060 changed";
  initRepo(repoRoot);
  writeRepoFile(repoRoot, primaryFile, `${baseLines.join("\n")}\n`);
  commitAll(repoRoot, "base");
  writeRepoFile(repoRoot, primaryFile, `${updatedLines.join("\n")}\n`);

  return {
    name: "small",
    repoRoot,
    repoName: "small-repo",
    primaryFile,
    secondaryFile: null,
    expectedPrimaryRowText: "line 060 changed",
    expectedSecondaryRowText: null,
    recipe: diffRecipe(repoRoot),
  };
}

function buildTwoSmallFilesFixture(root: string): FixtureEntry {
  const repoRoot = join(root, "two-small-files-repo");
  const primaryFile = "alpha.txt";
  const secondaryFile = "beta.txt";
  const expectedPrimaryRowText = "alpha uniquely changed";
  const expectedSecondaryRowText = "beta uniquely changed";
  initRepo(repoRoot);
  writeRepoFile(repoRoot, primaryFile, "alpha base\n");
  writeRepoFile(repoRoot, secondaryFile, "beta base\n");
  commitAll(repoRoot, "base");
  writeRepoFile(repoRoot, primaryFile, `${expectedPrimaryRowText}\n`);
  writeRepoFile(repoRoot, secondaryFile, `${expectedSecondaryRowText}\n`);

  return {
    name: "two-small-files",
    repoRoot,
    repoName: "two-small-files-repo",
    primaryFile,
    secondaryFile,
    expectedPrimaryRowText,
    expectedSecondaryRowText,
    recipe: diffRecipe(repoRoot),
  };
}

function buildLargeFileFixture(root: string): FixtureEntry {
  const repoRoot = join(root, "large-file-repo");
  const primaryFile = "large.txt";
  initRepo(repoRoot);
  writeRepoFile(repoRoot, primaryFile, "");
  commitAll(repoRoot, "base");
  writeRepoFile(repoRoot, primaryFile, repeatedLines("large line", LARGE_FILE_LINE_COUNT));

  return {
    name: "large-file",
    repoRoot,
    repoName: "large-file-repo",
    primaryFile,
    secondaryFile: null,
    expectedPrimaryRowText: "large line 00001",
    expectedSecondaryRowText: null,
    recipe: diffRecipe(repoRoot),
  };
}

function buildManyFilesFixture(root: string): FixtureEntry {
  const repoRoot = join(root, "many-files-repo");
  initRepo(repoRoot);
  for (let index = 0; index < MANY_FILES_COUNT; index += 1) {
    const filePath = `files/file-${String(index + 1).padStart(3, "0")}.txt`;
    writeRepoFile(repoRoot, filePath, `base ${index + 1}\n`);
  }
  commitAll(repoRoot, "base");
  for (let index = 0; index < MANY_FILES_COUNT; index += 1) {
    const filePath = `files/file-${String(index + 1).padStart(3, "0")}.txt`;
    writeRepoFile(
      repoRoot,
      filePath,
      [`changed ${index + 1} alpha`, `changed ${index + 1} beta`, `changed ${index + 1} gamma`].join("\n") + "\n",
    );
  }

  return {
    name: "many-files",
    repoRoot,
    repoName: "many-files-repo",
    primaryFile: "files/file-001.txt",
    secondaryFile: null,
    expectedPrimaryRowText: "changed 1 alpha",
    expectedSecondaryRowText: null,
    recipe: diffRecipe(repoRoot),
  };
}

/** A live-view healthy source: a real repo with a second local commit (unpushed, no remote
 * configured) plus a source path that is deliberately never created on disk (broken). */
function buildLiveViewFixtures(root: string): LiveViewFixtures {
  const healthyRoot = join(root, "live-view-repo");
  initRepo(healthyRoot);
  writeRepoFile(healthyRoot, "README.md", "hello\n");
  commitAll(healthyRoot, "base");
  writeRepoFile(healthyRoot, "README.md", "hello\nunpushed work\n");
  commitAll(healthyRoot, "unpushed work");

  return {
    healthy: { sourceKind: "LocalRepo", sourceValue: healthyRoot, displayName: "live-view-repo" },
    broken: {
      sourceKind: "LocalRepo",
      sourceValue: join(root, "missing-live-repo"),
      displayName: "missing-live-repo",
    },
  };
}

export function manifestPath(fixtureRoot: string): string {
  return join(resolve(fixtureRoot), MANIFEST_NAME);
}

export function buildFixtures(fixtureRoot: string): FixtureManifest {
  const root = resolve(fixtureRoot);
  rmSync(root, { recursive: true, force: true });
  mkdirSync(root, { recursive: true });

  const manifest: FixtureManifest = {
    generatedAt: new Date().toISOString(),
    fixtures: [
      buildSmallFixture(root),
      buildTwoSmallFilesFixture(root),
      buildLargeFileFixture(root),
      buildManyFilesFixture(root),
    ],
    liveViews: buildLiveViewFixtures(root),
  };

  writeFileSync(manifestPath(root), JSON.stringify(manifest, null, 2), "utf8");
  return manifest;
}

export function loadFixtureManifest(fixtureRoot = requiredEnv("GTL_NATIVE_PERF_FIXTURE_ROOT")): FixtureManifest {
  const manifest = JSON.parse(readFileSync(manifestPath(fixtureRoot), "utf8")) as unknown;
  if (!isFixtureManifest(manifest)) {
    throw new Error(`Malformed native perf fixture manifest at ${manifestPath(fixtureRoot)}`);
  }
  return manifest;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isFixtureEntry(value: unknown): value is FixtureEntry {
  if (!isObjectRecord(value)) return false;
  return (
    (value.name === "small" ||
      value.name === "two-small-files" ||
      value.name === "large-file" ||
      value.name === "many-files") &&
    typeof value.repoRoot === "string" &&
    typeof value.repoName === "string" &&
    typeof value.primaryFile === "string" &&
    (typeof value.secondaryFile === "string" || value.secondaryFile === null) &&
    typeof value.expectedPrimaryRowText === "string" &&
    (typeof value.expectedSecondaryRowText === "string" || value.expectedSecondaryRowText === null) &&
    isObjectRecord(value.recipe)
  );
}

function isLiveViewFixture(value: unknown): value is LiveViewFixture {
  if (!isObjectRecord(value)) return false;
  return (
    value.sourceKind === "LocalRepo" && typeof value.sourceValue === "string" && typeof value.displayName === "string"
  );
}

function isLiveViewFixtures(value: unknown): value is LiveViewFixtures {
  if (!isObjectRecord(value)) return false;
  return isLiveViewFixture(value.healthy) && isLiveViewFixture(value.broken);
}

function isFixtureManifest(value: unknown): value is FixtureManifest {
  if (!isObjectRecord(value)) return false;
  return (
    typeof value.generatedAt === "string" &&
    Array.isArray(value.fixtures) &&
    value.fixtures.every(isFixtureEntry) &&
    isLiveViewFixtures(value.liveViews)
  );
}

function main(): void {
  buildFixtures(requiredEnv("GTL_NATIVE_PERF_FIXTURE_ROOT"));
}

if (import.meta.main) {
  main();
}
