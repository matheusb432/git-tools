import path from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const executableSuffix = process.platform === 'win32' ? '.exe' : '';
const application = path.join(root, 'target', 'release', `gtl-viewer${executableSuffix}`);
const dataRoot = process.env.GTL_E2E_DATA_ROOT;
const tauriServicePackage = JSON.parse(
  readFileSync(path.join(here, 'node_modules/@wdio/tauri-service/package.json'), 'utf8'),
);

if (!dataRoot) {
  throw new Error('GTL_E2E_DATA_ROOT must be set by the xtask e2e harness');
}
if (tauriServicePackage.version !== '1.1.0') {
  throw new Error(
    `review the optional-plugin focus-probe guard for @wdio/tauri-service ${tauriServicePackage.version}`,
  );
}

export const config = {
  runner: 'local',
  specs: ['./spec/**/*.e2e.mjs'],
  maxInstances: 1,
  logLevel: 'error',
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    timeout: 120_000,
  },
  waitforTimeout: 30_000,
  connectionRetryTimeout: 120_000,
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application,
      },
      'wdio:tauriServiceOptions': {
        appBinaryPath: application,
        driverProvider: 'external',
        env: {
          GIT_TOOLS_DATA_DIR: dataRoot,
        },
      },
    },
  ],
  services: [
    [
      '@wdio/tauri-service',
      {
        appBinaryPath: application,
        driverProvider: 'external',
        autoInstallTauriDriver: false,
        env: {
          GIT_TOOLS_DATA_DIR: dataRoot,
        },
      },
    ],
  ],
  beforeTest() {
    // The Tauri service's focus hook probes the optional IPC plugin before every
    // element lookup. This suite intentionally uses the external provider with
    // no production plugin, so fail that optional probe immediately.
    if (typeof browser.tauri?.execute !== 'function') {
      throw new Error('@wdio/tauri-service no longer exposes browser.tauri.execute');
    }
    browser.tauri.execute = async () => {
      throw new Error('optional tauri-plugin-wdio is intentionally absent');
    };
  },
};
