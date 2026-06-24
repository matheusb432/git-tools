// Minimal DOM stub so Lit's module-level initialisation doesn't crash in bun's test runner.
// Lit reads globalThis.document at import time to create a tree walker used during template
// compilation. Providing a no-op version is enough for the unit tests in this directory,
// which only exercise pure logic functions (extractCopyText, scrollLandOn, copyText) and
// never render Lit templates.
if (typeof globalThis.document === "undefined") {
  const noop = (): object => ({ nextNode: () => null });
  // @ts-expect-error stub
  globalThis.document = {
    createTreeWalker: noop,
    createComment: () => ({}),
    createElement: () => ({ style: {}, select() {} }),
    body: { appendChild() {}, removeChild() {} },
    execCommand: () => false,
    documentElement: { dataset: {} },
    querySelectorAll: () => [],
    addEventListener: () => {},
  };
}
if (typeof globalThis.window === "undefined") {
  // @ts-expect-error stub
  globalThis.window = { getSelection: () => null, CSS: undefined, innerHeight: 0 };
}
if (typeof globalThis.navigator === "undefined") {
  // @ts-expect-error stub
  globalThis.navigator = {};
}
