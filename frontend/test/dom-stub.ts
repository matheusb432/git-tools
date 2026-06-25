// Minimal DOM stub for bun's test runner — provides just enough surface for the unit tests
// in this directory, which exercise pure logic functions (extractCopyText, scrollLandOn,
// copyText) and never render any component templates.
if (typeof globalThis.document === "undefined") {
  const noop = (): object => ({ nextNode: () => null });
  globalThis.document = {
    createTreeWalker: noop,
    createComment: () => ({}),
    createElement: () => ({ style: {}, select() {} }),
    body: { appendChild() {}, removeChild() {} },
    execCommand: () => false,
    documentElement: { dataset: {} },
    querySelectorAll: () => [],
    addEventListener: () => {},
  } as unknown as Document;
}
if (typeof globalThis.window === "undefined") {
  globalThis.window = { getSelection: () => null, CSS: undefined, innerHeight: 0 } as unknown as Window & typeof globalThis;
}
if (typeof globalThis.navigator === "undefined") {
  globalThis.navigator = {} as unknown as Navigator;
}
