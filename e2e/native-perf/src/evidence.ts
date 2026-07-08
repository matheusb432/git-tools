import { cpus, release, totalmem, version } from "node:os";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";

type ProcessEnv = Record<string, string | undefined>;

declare const browser: {
  readonly saveScreenshot: (filePath: string) => Promise<void>;
  readonly execute: <T>(script: () => T) => Promise<T>;
};

type TimingRecord = {
  readonly name: string;
  readonly ms?: number;
  readonly gateMs?: number;
  readonly value?: number;
  readonly unit?: "ms" | "fps";
  readonly minimum?: number;
  readonly note?: string;
};

type DomCountRecord = {
  readonly name: string;
  readonly count: number;
  readonly limit?: number;
  readonly note?: string;
};

export type EnvironmentEvidence = {
  readonly capturedAt: string;
  readonly git: {
    readonly commit: string | null;
    readonly branch: string | null;
    readonly dirty: boolean | null;
  };
  readonly machine: {
    readonly machineId: string | null;
    readonly cpu: {
      readonly model: string | null;
      readonly logicalCores: number;
      readonly speedMhz: number | null;
    };
    readonly memory: {
      readonly totalBytes: number;
    };
  };
  readonly os: {
    readonly platform: NodeJS.Platform;
    readonly arch: NodeJS.Architecture;
    readonly kernelRelease: string;
    readonly version: string | null;
    readonly distro: OsReleaseInfo | null;
  };
  readonly runtime: {
    readonly node: string;
    readonly bun: string | null;
  };
  readonly platform: NodeJS.Platform;
  readonly arch: NodeJS.Architecture;
  readonly node: string;
  readonly bun: string | null;
  readonly smoke: boolean;
  readonly appBinary: string | null;
  readonly tauriDriver: string | null;
  readonly fixtureRoot: string | null;
  readonly dataDir: string | null;
  readonly userAgent: string;
  readonly screenshots: readonly string[];
  readonly capabilities: unknown;
};

type OsReleaseInfo = {
  readonly id: string | null;
  readonly name: string | null;
  readonly versionId: string | null;
  readonly prettyName: string | null;
};

type EnvironmentEvidenceInput = {
  readonly capabilities: unknown;
  readonly userAgent: string;
  readonly screenshots: readonly string[];
  readonly env?: ProcessEnv;
  readonly capturedAt?: string;
};

const timings: TimingRecord[] = [];
const domCounts: DomCountRecord[] = [];
const screenshots: string[] = [];

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}

function artifactDir(): string | null {
  const value = process.env.GTL_NATIVE_PERF_ARTIFACT_DIR;
  return value === undefined || value.trim() === "" ? null : resolve(value);
}

function ensureArtifactDir(): string | null {
  const dir = artifactDir();
  if (dir === null) return null;
  mkdirSync(dir, { recursive: true });
  return dir;
}

function slug(value: string): string {
  return (
    value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 80) || "artifact"
  );
}

function writeJson(filePath: string, value: unknown): void {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

export function evidenceEnabled(): boolean {
  return process.env.GTL_NATIVE_PERF_EVIDENCE === "1";
}

export function pruneEvidenceRoots(): void {
  if (!evidenceEnabled()) return;
  const repoRoot = requiredEnv("GTL_NATIVE_PERF_REPO_ROOT");
  rmSync(join(repoRoot, ".artifacts/e2e/success"), { recursive: true, force: true });
  rmSync(join(repoRoot, ".artifacts/e2e/fail"), { recursive: true, force: true });
}

export function recordTiming(record: TimingRecord): void {
  timings.push(record);
}

export function recordDomCount(record: DomCountRecord): void {
  domCounts.push(record);
}

export async function captureScreenshot(name: string): Promise<void> {
  if (!evidenceEnabled()) return;
  const dir = ensureArtifactDir();
  if (dir === null) return;
  const filePath = join(dir, `${slug(name)}.png`);
  await browser.saveScreenshot(filePath);
  screenshots.push(basename(filePath));
}

export async function captureFailureScreenshot(name: string): Promise<void> {
  await captureScreenshot(`${name}-failure`);
}

export function buildEnvironmentEvidence({
  capabilities,
  userAgent,
  screenshots,
  env = process.env,
  capturedAt = new Date().toISOString(),
}: EnvironmentEvidenceInput): EnvironmentEvidence {
  const cpuList = cpus();
  const cpu = cpuList[0];
  const node = process.version;
  const bun = process.versions.bun ?? null;

  return {
    capturedAt,
    git: {
      commit: optionalEnv(env, "GTL_NATIVE_PERF_GIT_COMMIT"),
      branch: optionalEnv(env, "GTL_NATIVE_PERF_GIT_BRANCH"),
      dirty: optionalBooleanEnv(env, "GTL_NATIVE_PERF_GIT_DIRTY"),
    },
    machine: {
      machineId: optionalEnv(env, "GTL_NATIVE_PERF_MACHINE_ID"),
      cpu: {
        model: cpu?.model ?? null,
        logicalCores: cpuList.length,
        speedMhz: cpu?.speed ?? null,
      },
      memory: {
        totalBytes: totalmem(),
      },
    },
    os: {
      platform: process.platform,
      arch: process.arch,
      kernelRelease: release(),
      version: version(),
      distro: readOsRelease(),
    },
    runtime: {
      node,
      bun,
    },
    platform: process.platform,
    arch: process.arch,
    node,
    bun,
    smoke: env.GTL_NATIVE_PERF_SMOKE === "1",
    appBinary: env.GTL_NATIVE_PERF_APP_BINARY ?? null,
    tauriDriver: env.GTL_NATIVE_PERF_TAURI_DRIVER ?? null,
    fixtureRoot: env.GTL_NATIVE_PERF_FIXTURE_ROOT ?? null,
    dataDir: env.GIT_TOOLS_DATA_DIR ?? null,
    userAgent,
    screenshots: [...screenshots],
    capabilities,
  };
}

export async function flushEvidence(capabilities: unknown): Promise<void> {
  if (!evidenceEnabled()) return;
  const dir = ensureArtifactDir();
  if (dir === null) return;

  writeJson(join(dir, "timings.json"), timings);
  writeJson(join(dir, "dom-counts.json"), domCounts);

  const userAgent = await browser.execute(() => navigator.userAgent);
  writeJson(
    join(dir, "environment.json"),
    buildEnvironmentEvidence({
      capabilities,
      userAgent,
      screenshots,
    }),
  );
}

function optionalEnv(env: ProcessEnv, name: string): string | null {
  const value = env[name];
  if (value === undefined) return null;
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

function optionalBooleanEnv(env: ProcessEnv, name: string): boolean | null {
  const value = optionalEnv(env, name);
  if (value === null) return null;
  if (value === "1" || value === "true") return true;
  if (value === "0" || value === "false") return false;
  return null;
}

function readOsRelease(): OsReleaseInfo | null {
  try {
    const contents = readFileSync("/etc/os-release", "utf8");
    const values = parseOsRelease(contents);
    return {
      id: values.ID ?? null,
      name: values.NAME ?? null,
      versionId: values.VERSION_ID ?? null,
      prettyName: values.PRETTY_NAME ?? null,
    };
  } catch {
    return null;
  }
}

function parseOsRelease(contents: string): Record<string, string> {
  const values: Record<string, string> = {};
  for (const line of contents.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    const separatorIndex = trimmed.indexOf("=");
    if (separatorIndex === -1) continue;
    const key = trimmed.slice(0, separatorIndex);
    const rawValue = trimmed.slice(separatorIndex + 1);
    values[key] = unquoteOsReleaseValue(rawValue);
  }
  return values;
}

function unquoteOsReleaseValue(value: string): string {
  if (value.length < 2) return value;
  const quote = value[0];
  if ((quote !== '"' && quote !== "'") || value[value.length - 1] !== quote) {
    return value;
  }
  return value.slice(1, -1).replaceAll(`\\${quote}`, quote);
}
