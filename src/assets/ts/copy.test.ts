import { expect, test } from "bun:test";
import { extractCopyText } from "./components";

// Row descriptor parsed from a minimal XML-like mini-DSL:
//   <row code="+const x = 1" lns="0,12"/>          → normal row; code is raw marker+text
//   <row codeText="+data:font/woff;base64,AAAA" lns="0,1"/>  → row with .code-text child
type RowDesc =
  | { kind: "code"; marker: string; text: string; lns: [number, number] }
  | { kind: "codeText"; marker: string; text: string; lns: [number, number] };

function parseRows(html: string): RowDesc[] {
  const rows: RowDesc[] = [];
  // Match <row ... /> where attr values may contain '/' (e.g. data: URIs)
  const re = /<row\s+((?:[a-zA-Z-]+="[^"]*"\s*)*)\s*\/>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(html)) !== null) {
    const attrs = m[1] ?? "";
    const getAttr = (name: string): string | undefined => {
      const am = new RegExp(`${name}="([^"]*)"`, "").exec(attrs);
      return am ? am[1] : undefined;
    };
    const lnsRaw = getAttr("lns") ?? "0,0";
    const parts = lnsRaw.split(",");
    const lns: [number, number] = [parseInt(parts[0] ?? "0", 10), parseInt(parts[1] ?? "0", 10)];
    const code = getAttr("code");
    const codeText = getAttr("codeText");
    if (code !== undefined) {
      rows.push({ kind: "code", marker: code[0] ?? "", text: code.slice(1), lns });
    } else if (codeText !== undefined) {
      rows.push({ kind: "codeText", marker: codeText[0] ?? "", text: codeText.slice(1), lns });
    }
  }
  return rows;
}

function makeLnEl(n: number): object {
  return { textContent: String(n) };
}

function makeRowEl(desc: RowDesc): object {
  const [oldLn, newLn] = desc.lns;
  const lnEls = [makeLnEl(oldLn), makeLnEl(newLn)];
  const rawText = desc.marker + desc.text;

  let codeEl: object;
  if (desc.kind === "codeText") {
    const codeTextEl = { textContent: rawText };
    codeEl = {
      textContent: rawText + "[expand]", // simulates extra label text on the outer code
      querySelector: (sel: string): object | null => sel === ".code-text" ? codeTextEl : null,
    };
  } else {
    codeEl = {
      textContent: rawText,
      querySelector: (_sel: string): object | null => null,
    };
  }

  return {
    querySelector: (sel: string): object | null => sel === "code" ? codeEl : null,
    querySelectorAll: (sel: string): object[] => sel === ".ln" ? lnEls : [],
  };
}

function makeFileStub(rowsHtml: string, attrs: Record<string, string> = {}): object {
  const descs = parseRows(rowsHtml);
  const rowEls = descs.map(makeRowEl);

  // The .layout element: present when attrs has "copy-ctx"
  const hasCopyCtx = "copy-ctx" in attrs;
  const layoutEl = {
    classList: {
      contains: (cls: string): boolean => cls === "copy-ctx" ? hasCopyCtx : false,
    },
  };

  return {
    querySelectorAll: (sel: string): object[] => {
      // Match .diff:not([hidden]) .dl-add, .diff:not([hidden]) .dl-ctx
      if (sel.includes("dl-add") || sel.includes("dl-ctx")) return rowEls;
      return [];
    },
    closest: (sel: string): object | null => sel === ".layout" ? layoutEl : null,
    getAttribute: (name: string): string | null => attrs[name] ?? null,
  };
}

test("extractCopyText reads .code-text (not the expander label) and strips markers", () => {
  const file = makeFileStub(
    `<row code="+const x = 1" lns="0,12"/>`,
    { "data-comment": "//", "data-path": "src/a.ts", "copy-ctx": "true" },
  );
  expect(extractCopyText(file as Element)).toBe("// * src/a.ts, lines: 12..12\nconst x = 1");
});

test("extractCopyText prefers .code-text over code when a long line is split-clipped", () => {
  const file = makeFileStub(`<row codeText="+data:font/woff;base64,AAAA" lns="0,1"/>`, { "data-path": "f.css" });
  expect(extractCopyText(file as Element)).toContain("data:font/woff;base64,AAAA");
});
