import { expect, test } from "vitest";
import { extractCopyText } from "./copy";

type RowDesc = {
  /** Rendered marker column: "+" for additions, " " for context. */
  readonly marker: string;
  readonly text: string;
  readonly newLine: number;
  /** Long-line rows nest the raw text in .code-text while <code> also carries the expander. */
  readonly expander?: string;
};

function makeRowEl(desc: RowDesc): object {
  const lnEls = [{ textContent: "0" }, { textContent: String(desc.newLine) }];
  const raw = desc.marker + desc.text;
  const codeTextEl = { textContent: raw };
  const codeEl =
    desc.expander === undefined
      ? { textContent: raw, querySelector: (): object | null => null }
      : {
          textContent: raw + desc.expander,
          querySelector: (selector: string): object | null => (selector === ".code-text" ? codeTextEl : null),
        };

  return {
    querySelector: (selector: string): object | null => (selector === "code" ? codeEl : null),
    querySelectorAll: (selector: string): object[] => (selector === ".ln" ? lnEls : []),
  };
}

function makeFileStub(rows: readonly RowDesc[], attrs: Record<string, string>, contextOn: boolean): object {
  const rowEls = rows.map(makeRowEl);
  const layoutEl = {
    classList: { contains: (name: string): boolean => name === "copy-ctx" && contextOn },
  };
  const unifiedEl = {
    querySelectorAll: (selector: string): object[] => (selector.includes("dl-add") ? rowEls : []),
  };

  return {
    querySelector: (selector: string): object | null => (selector === ".diff-unified" ? unifiedEl : null),
    closest: (selector: string): object | null => (selector === ".layout" ? layoutEl : null),
    getAttribute: (name: string): string | null => attrs[name] ?? null,
  };
}

function makeSplitFileStub(attrs: Record<string, string>): object {
  const lineNumber = { textContent: "12", matches: (selector: string) => selector === ".ln" };
  const code = {
    textContent: "+const x = 1",
    previousElementSibling: lineNumber,
    querySelector: (): object | null => null,
  };
  const split = {
    querySelectorAll: (selector: string): object[] => (selector === ".sp-add, .dl > .sp-ctx:last-child" ? [code] : []),
  };
  const layoutEl = {
    classList: { contains: (name: string): boolean => name === "copy-ctx" },
  };

  return {
    querySelector: (selector: string): object | null => (selector === ".diff-split" ? split : null),
    closest: (selector: string): object | null => (selector === ".layout" ? layoutEl : null),
    getAttribute: (name: string): string | null => attrs[name] ?? null,
  };
}

test("extractCopyText headers the copy with the span the rows cover", () => {
  const multi = makeFileStub(
    [
      { marker: "+", text: "const x = 1", newLine: 12 },
      { marker: "+", text: "const y = 2", newLine: 13 },
    ],
    { "data-comment": "//", "data-path": "src/a.ts" },
    true,
  );

  expect(extractCopyText(multi as Element)).toBe("// * src/a.ts, lines: 12..13\nconst x = 1\nconst y = 2");
});

test("extractCopyText prefers .code-text over code when a long line is split-clipped", () => {
  const file = makeFileStub(
    [{ marker: "+", text: "data:font/woff;base64,AAAA", newLine: 1, expander: "[expand]" }],
    { "data-path": "f.css" },
    false,
  );

  expect(extractCopyText(file as Element)).toBe("data:font/woff;base64,AAAA");
});

test("extractCopyText reads the rendered split pane when no unified pane exists", () => {
  const file = makeSplitFileStub({ "data-comment": "//", "data-path": "src/a.ts" });

  expect(extractCopyText(file as Element)).toBe("// * src/a.ts, lines: 12\nconst x = 1");
});
