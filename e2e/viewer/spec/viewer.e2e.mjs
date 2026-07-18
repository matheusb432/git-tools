import { execFile } from 'node:child_process';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const runFile = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const executableSuffix = process.platform === 'win32' ? '.exe' : '';
const cli = path.join(root, 'target', 'release', `git-tools${executableSuffix}`);
const dataRoot = process.env.GTL_E2E_DATA_ROOT;
const sandboxFixtures = process.env.GTL_E2E_FIXTURE_ROOT;

if (!sandboxFixtures) {
  throw new Error('GTL_E2E_FIXTURE_ROOT must be set by the xtask e2e harness');
}

let fixtureRoot;
let remoteRoot;
let primaryRepo;
let secondaryRepo;

async function run(program, args, cwd) {
  return runFile(program, args, {
    cwd,
    env: {
      ...process.env,
      GIT_TOOLS_DATA_DIR: dataRoot,
    },
  });
}

async function git(cwd, ...args) {
  return run('git', args, cwd);
}

async function pressEnter() {
  if (process.platform === 'linux') {
    await run('xdotool', ['key', 'Return'], root);
    return;
  }
  await browser.keys('Enter');
}

async function createRepo(name, content) {
  const repo = path.join(fixtureRoot, name);
  await mkdir(repo, { recursive: true });
  await git(repo, 'init', '-q', '-b', 'main');
  await git(repo, 'config', 'user.name', 'Viewer E2E');
  await git(repo, 'config', 'user.email', 'viewer-e2e@example.invalid');
  await writeFile(path.join(repo, 'work.txt'), content);
  await git(repo, 'add', 'work.txt');
  await git(repo, 'commit', '-q', '-m', 'base');
  const remote = path.join(remoteRoot, `${name}.git`);
  await mkdir(remote, { recursive: true });
  await git(remote, 'init', '--bare', '-q');
  await git(repo, 'remote', 'add', 'origin', remote);
  await git(repo, 'push', '-q', '-u', 'origin', 'main');
  return repo;
}

async function openDiff(repo, expectedTabs, ...args) {
  await run(cli, ['diff', ...args], repo);
  await drainPending({ minTabs: expectedTabs });
}

async function drainPending(expected) {
  await browser.waitUntil(
    async () =>
      browser.execute(async (condition) => {
        await window.htmx.ajax('GET', '/pending', {
          target: '#viewer-tabs',
          swap: 'outerHTML',
        });
        const tabCount = document.querySelectorAll('.viewer-tab').length;
        const activeKind = document.querySelector(
          '.viewer-tab.active .viewer-tab-kind',
        )?.textContent;
        return (
          tabCount >= (condition.minTabs ?? 0) &&
          (!condition.activeKind || activeKind === condition.activeKind)
        );
      }, expected),
    {
      timeout: 30_000,
      interval: 500,
      timeoutMsg: `pending recipe batch did not reach the viewer: ${JSON.stringify(expected)}`,
    },
  );
}

async function expectReadyDocument() {
  try {
    await expect($('#viewer-view .layout')).toBeDisplayed();
    await expect($('#viewer-view iframe')).not.toExist();
  } catch (error) {
    console.error(
      await browser.execute(() => {
        const view = document.querySelector('#viewer-view');
        const layout = document.querySelector('#viewer-view .layout');
        return {
          view: view?.outerHTML.slice(0, 500),
          viewRect: view?.getBoundingClientRect().toJSON(),
          viewDisplay: view ? getComputedStyle(view).display : null,
          layout: layout?.outerHTML.slice(0, 500),
          layoutRect: layout?.getBoundingClientRect().toJSON(),
          layoutDisplay: layout ? getComputedStyle(layout).display : null,
          visibility: layout ? getComputedStyle(layout).visibility : null,
        };
      }),
    );
    throw error;
  }
}

async function waitForHtmxIdle(label) {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => !document.querySelector('.hx-request, .htmx-request'),
      ),
    { timeoutMsg: `htmx did not settle after ${label}` },
  );
}

async function selectSplitLayout() {
  await browser.execute(() => {
    const input = document.querySelector("input[name='viewer-layout'][value='split']");
    if (!(input instanceof HTMLInputElement)) {
      throw new Error('split layout input is missing');
    }
    input.click();
  });
  await waitForHtmxIdle('selecting the split layout');
  await expect($('#viewer-view .diff-split')).toExist();
  await expect($("input[name='viewer-layout'][value='split']")).toBeChecked();
}

async function closeActiveTab() {
  const before = await browser.execute(
    () => document.querySelectorAll('.viewer-tab').length,
  );
  await $('.viewer-tab.active .viewer-tab-close').click();
  await browser.waitUntil(
    async () =>
      browser.execute(
        (previous) => document.querySelectorAll('.viewer-tab').length < previous,
        before,
      ),
    { timeoutMsg: 'active tab did not close' },
  );
  await waitForHtmxIdle('closing the active tab');
}

async function closeAllTabs() {
  while (
    (await browser.execute(() => document.querySelectorAll('.viewer-tab').length)) > 0
  ) {
    await closeActiveTab();
  }
}

describe('server-rendered viewer', () => {
  before(async () => {
    fixtureRoot = path.join(sandboxFixtures, 'dom-repositories');
    remoteRoot = path.join(sandboxFixtures, 'dom-remotes');
    await mkdir(fixtureRoot, { recursive: true });
    await mkdir(remoteRoot, { recursive: true });
    primaryRepo = await createRepo('primary', 'base\n');
    secondaryRepo = await createRepo('secondary', 'base\n');
    await createRepo('empty', 'base\n');
    await writeFile(path.join(primaryRepo, 'work.txt'), 'base\nalpha-v1\n');
    await writeFile(path.join(secondaryRepo, 'work.txt'), 'base\nbeta-v1\n');
    for (const repo of [primaryRepo, secondaryRepo]) {
      await git(repo, 'add', 'work.txt');
      await git(repo, 'commit', '-q', '-m', 'unpushed work');
    }
  });

  after(async () => {
    if (fixtureRoot) {
      await rm(fixtureRoot, { recursive: true, force: true });
    }
    if (remoteRoot) {
      await rm(remoteRoot, { recursive: true, force: true });
    }
  });

  it('opens a CLI-forwarded recipe as a server-rendered document', async () => {
    await openDiff(primaryRepo, 1, '--name', 'primary snapshot');

    await expectReadyDocument();
    await expect($('#viewer-view')).toHaveText(expect.stringContaining('alpha-v1'));
  });

  it('persists layout across a real WebDriver session restart', async () => {
    await selectSplitLayout();

    await browser.reloadSession();
    await openDiff(primaryRepo, 1, '--name', 'primary after restart');

    await expect($('#viewer-view .diff-split')).toExist();
    await expect($("input[name='viewer-layout'][value='split']")).toBeChecked();
  });

  it('reopens history by its stable persisted id', async () => {
    await closeActiveTab();
    await expect($('.viewer-status-empty')).toBeDisplayed();

    await $('.viewer-history-button').click();
    const historyRow = $('.viewer-history-row');
    await expect(historyRow).toBeDisplayed();
    await expect(historyRow).toHaveAttribute('hx-get', expect.stringMatching(/^\/history\/\d+\/open$/));
    await historyRow.click();

    await expectReadyDocument();
  });

  it('accepts a recursive batch while reporting clean repositories', async () => {
    await run(cli, ['diff', '-r'], fixtureRoot);

    await browser.waitUntil(
      async () =>
        browser.execute(
          () => document.querySelectorAll('.viewer-tab').length >= 3,
        ),
      {
        timeout: 30_000,
        interval: 100,
        timeoutMsg: 'recursive batch did not reach the viewer',
      },
    );

    const batchState = await browser.execute(() => ({
      tabs: Array.from(document.querySelectorAll('.viewer-tab')).map((tab) => ({
        label: tab.querySelector('.viewer-tab-label')?.textContent,
        state: tab.querySelector('.viewer-tab-state')?.getAttribute('aria-label') ?? 'ready',
      })),
    }));
    expect(batchState.tabs).toHaveLength(3);
    const toast = $('[data-viewer-toast]');
    await expect(toast).toBeDisplayed();
    await expect(toast).toHaveText(expect.stringContaining('empty'));
    const labels = batchState.tabs.map((tab) => tab.label);
    expect(labels).toEqual(expect.arrayContaining(['primary', 'secondary']));
    const activeLabel = await browser.execute(
      () => document.querySelector('.viewer-tab.active .viewer-tab-label')?.textContent,
    );
    expect(activeLabel).toContain('secondary');
    await expectReadyDocument();
    await browser.waitUntil(
      async () =>
        browser.execute(
          () => getComputedStyle(document.querySelector('[data-viewer-toast]')).opacity === '0',
        ),
      { timeout: 8_000, timeoutMsg: 'skipped-diff toast did not dismiss' },
    );
  });

  it('refreshes a live source and distinguishes a subsequently broken source', async () => {
    await closeAllTabs();
    await run(cli, ['diff', 'live', '--path', primaryRepo], root);
    await drainPending({ minTabs: 1, activeKind: 'L' });
    await expectReadyDocument();

    await writeFile(path.join(primaryRepo, 'work.txt'), 'base\nalpha-v2\n');
    await git(primaryRepo, 'add', 'work.txt');
    await git(primaryRepo, 'commit', '-q', '-m', 'refresh work');
    await $('button=Refresh').click();
    await expect($('#viewer-view')).toHaveText(expect.stringContaining('alpha-v2'));

    await rm(primaryRepo, { recursive: true, force: true });
    await $('button=Refresh').click();
    await expect($('.viewer-status-broken')).toBeDisplayed();
    await expect($('.viewer-status-error')).not.toExist();

    const deleteLiveView = $('button=Delete live view');
    await browser.execute((button) => button.focus(), await deleteLiveView);
    await pressEnter();
    expect(await browser.getAlertText()).toBe(
      'Delete this saved live view? This removes its tab and automatic restoration. You can add it again with gtl diff live.',
    );
    await browser.acceptAlert();
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document.activeElement?.classList.contains('viewer-recovery-button') &&
            document.activeElement?.textContent?.trim() === 'Open history',
        ),
      { timeoutMsg: 'focus did not move to the empty-state History action' },
    );
    expect(
      await browser.execute(
        () => document.querySelector('.viewer-sr-only[role="status"]')?.textContent?.trim(),
      ),
    ).toContain(
      'Live view deleted. No diffs remain open.',
    );
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            !Array.from(document.querySelectorAll('.viewer-tab-kind')).some(
              (kind) => kind.textContent === 'L',
            ),
        ),
      { timeoutMsg: 'deleted live view remained in the authoritative tab strip' },
    );

    await browser.reloadSession();
    await expect($('.viewer-status-empty')).toBeDisplayed();
    await expect($('button=Delete live view')).not.toExist();
  });
});
