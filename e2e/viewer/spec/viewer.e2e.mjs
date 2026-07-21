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
let repository;

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

async function createRepo() {
  const localRepository = path.join(fixtureRoot, 'live-view');
  await mkdir(localRepository, { recursive: true });
  await git(localRepository, 'init', '-q', '-b', 'main');
  await git(localRepository, 'config', 'user.name', 'Viewer E2E');
  await git(localRepository, 'config', 'user.email', 'viewer-e2e@example.invalid');
  await writeFile(path.join(localRepository, 'work.txt'), 'base\n');
  await git(localRepository, 'add', 'work.txt');
  await git(localRepository, 'commit', '-q', '-m', 'base');
  await git(localRepository, 'switch', '-q', '-c', 'feature');
  await writeFile(path.join(localRepository, 'work.txt'), 'base\nalpha-v1\n');
  await git(localRepository, 'add', 'work.txt');
  await git(localRepository, 'commit', '-q', '-m', 'live view v1');
  return localRepository;
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

describe('server-rendered viewer', () => {
  before(async () => {
    fixtureRoot = path.join(sandboxFixtures, 'dom-repositories');
    repository = await createRepo();
  });

  after(async () => {
    if (fixtureRoot) {
      await rm(fixtureRoot, { recursive: true, force: true });
    }
  });

  it('forwarded live view restores refreshes and deletes', async () => {
    await run(cli, ['diff', 'live', '--path', repository], root);
    await drainPending({ minTabs: 1, activeKind: 'L' });
    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('.viewer-tab').length === 1),
      { timeoutMsg: 'the forwarded live view did not become the only active tab' },
    );
    await expectReadyDocument();
    await expect($('#viewer-view')).toHaveText(expect.stringContaining('alpha-v1'));

    await selectSplitLayout();

    await browser.reloadSession();
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document.querySelectorAll('.viewer-tab').length === 1 &&
            document.querySelector('.viewer-tab.active .viewer-tab-kind')?.textContent === 'L' &&
            document.querySelector('#viewer-view .diff-split') !== null &&
            document.querySelector("input[name='viewer-layout'][value='split']")?.checked === true,
        ),
      { timeout: 30_000, timeoutMsg: 'the live view and split layout did not restore' },
    );

    await writeFile(path.join(repository, 'work.txt'), 'base\nalpha-v2\n');
    await git(repository, 'add', 'work.txt');
    await git(repository, 'commit', '-q', '-m', 'live view v2');
    await $('button=Refresh').click();
    await waitForHtmxIdle('refreshing the live view');
    await expect($('#viewer-view')).toHaveText(expect.stringContaining('alpha-v2'));

    const deleteLiveView = $('button=Delete live view');
    await browser.execute((button) => button.focus(), await deleteLiveView);
    await pressEnter();
    expect(await browser.getAlertText()).toBe(
      'Delete this saved live view? This removes its tab and automatic restoration. You can add it again with gtl diff live.',
    );
    await browser.acceptAlert();
    await waitForHtmxIdle('deleting the live view');
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            !Array.from(document.querySelectorAll('.viewer-tab-kind')).some(
              (kind) => kind.textContent === 'L',
            ) &&
            document.querySelector('.viewer-status-empty') !== null &&
            document.activeElement?.classList.contains('viewer-recovery-button'),
        ),
      { timeoutMsg: 'the deleted live view did not transition to the focused empty state' },
    );
    await expect($('.viewer-status-empty')).toBeDisplayed();
    await expect($('.viewer-recovery-button')).toBeFocused();

    await browser.reloadSession();
    await expect($('.viewer-status-empty')).toBeDisplayed();
    await expect($('button=Delete live view')).not.toExist();
  });
});
