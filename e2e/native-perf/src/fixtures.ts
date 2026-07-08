import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import type { Recipe } from "../../../frontend/viewer/shared/api/index";

const MANIFEST_NAME = "manifest.json";
const LARGE_FILE_LINE_COUNT = 45_000;
const MANY_FILES_COUNT = 300;
const decoder = new TextDecoder();

export type FixtureName = "small" | "large-file" | "many-files";

export type FixtureEntry = {
  readonly name: FixtureName;
  readonly repoRoot: string;
  readonly repoName: string;
  readonly primaryFile: string;
  readonly recipe: Recipe;
};

export type FixtureManifest = {
  readonly generatedAt: string;
  readonly fixtures: readonly FixtureEntry[];
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
      [
        `changed ${index + 1} alpha`,
        `changed ${index + 1} beta`,
        `changed ${index + 1} gamma`,
      ].join("\n") + "\n",
    );
  }

  return {
    name: "many-files",
    repoRoot,
    repoName: "many-files-repo",
    primaryFile: "files/file-001.txt",
    recipe: diffRecipe(repoRoot),
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
      buildLargeFileFixture(root),
      buildManyFilesFixture(root),
    ],
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
    (value.name === "small" || value.name === "large-file" || value.name === "many-files") &&
    typeof value.repoRoot === "string" &&
    typeof value.repoName === "string" &&
    typeof value.primaryFile === "string" &&
    isObjectRecord(value.recipe)
  );
}

function isFixtureManifest(value: unknown): value is FixtureManifest {
  if (!isObjectRecord(value)) return false;
  return (
    typeof value.generatedAt === "string" &&
    Array.isArray(value.fixtures) &&
    value.fixtures.every(isFixtureEntry)
  );
}

function main(): void {
  buildFixtures(requiredEnv("GTL_NATIVE_PERF_FIXTURE_ROOT"));
}

if (import.meta.main) {
  main();
}
