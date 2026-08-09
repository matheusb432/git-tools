import { afterEach, describe, expect, test, vi } from "vitest";
import { DIFF_DOCUMENT_SELECTOR } from "../diff/enhance-document";
import { createDiffIslandAdapter, type PreparedDiffDocument } from "./adapter";

const DOCUMENT_HTML = "prepared-document";
const REPLACEMENT_HTML = "replacement-document";
const CHUNK_HTML = "trusted-chunk";

afterEach(() => {
  vi.restoreAllMocks();
});

function must<T>(value: T | null, what: string): T {
  if (value === null) throw new Error(`${what} is missing`);
  return value;
}

function diffDocument(path: string): HTMLElement {
  const root = document.createElement("main");
  root.setAttribute("data-gtl-diff-document", "");
  root.className = "main";
  const file = document.createElement("details");
  file.className = "file";
  file.id = "src-lib-rs";
  file.setAttribute("data-path", path);
  const target = document.createElement("div");
  target.id = "viewer-diff-0";
  target.className = "diff";
  file.appendChild(target);
  root.appendChild(file);
  return root;
}

function lateChunk(): HTMLElement {
  const chunk = document.createElement("div");
  const longRow = document.createElement("div");
  longRow.className = "dl-long";
  const longLineButton = document.createElement("button");
  longLineButton.className = "ln-more";
  longRow.appendChild(longLineButton);
  const copyButton = document.createElement("button");
  copyButton.className = "copy-button";
  copyButton.dataset["copyValue"] = "late chunk value";
  copyButton.dataset["copyLabel"] = "chunk";
  chunk.appendChild(longRow);
  chunk.appendChild(copyButton);
  return chunk;
}

function prepared(viewIdentity: string, html = DOCUMENT_HTML): PreparedDiffDocument {
  return {
    viewIdentity,
    html,
    styleHref: "/assets/diff-island.css",
    materialization: { kind: "loading" },
  };
}

function installOpenShadowRoot(host: HTMLElement): ShadowRoot {
  const shadow = document.createElement("div");
  Object.defineProperty(shadow, "getElementById", {
    value: (id: string) =>
      [...shadow.querySelectorAll<HTMLElement>("[id]")].find((element) => element.id === id) ?? null,
  });
  Object.defineProperty(host, "shadowRoot", { configurable: true, get: () => shadow });
  Object.defineProperty(host, "attachShadow", { configurable: true, value: () => shadow });
  return host.shadowRoot ?? host.attachShadow({ mode: "open" });
}

function installTrustedHtmlFixtures(fixtures: ReadonlyMap<string, () => readonly HTMLElement[]>): void {
  const createElement = document.createElement.bind(document);
  vi.spyOn(document, "createElement").mockImplementation((tagName) => {
    const element = createElement(tagName);
    if (tagName.toLowerCase() !== "template") return element;
    const content = createElement("div");
    let html = "";
    Object.defineProperty(element, "content", { value: content });
    Object.defineProperty(element, "innerHTML", {
      configurable: true,
      get: () => html,
      set: (value: string) => {
        html = value;
        content.replaceChildren(...(fixtures.get(value)?.() ?? []));
      },
    });
    return element;
  });
}

describe("createDiffIslandAdapter", () => {
  test("late trusted chunks inherit long-line and copy enhancement and complete the host", async () => {
    installTrustedHtmlFixtures(
      new Map([
        [DOCUMENT_HTML, () => [diffDocument("src/lib.rs")]],
        [CHUNK_HTML, () => [lateChunk()]],
      ]),
    );
    const host = document.createElement("section");
    installOpenShadowRoot(host);
    document.body.appendChild(host);
    let copied = "";
    vi.spyOn(document, "execCommand").mockImplementation(() => {
      copied = document.body.querySelector<HTMLTextAreaElement>("textarea")?.value ?? "";
      return true;
    });
    const island = createDiffIslandAdapter();
    const chain = island.mount(host, prepared("tab-1:range"));

    expect(
      island.appendChunk(chain, {
        viewIdentity: "tab-1:range",
        targetId: "viewer-diff-0",
        html: CHUNK_HTML,
        rowCount: 2,
        continuation: false,
      }),
    ).toBe("appended");
    const shadow = must(host.shadowRoot, "the open shadow root");
    const longLineButton = must(shadow.querySelector<HTMLButtonElement>(".ln-more"), "the late long-line button");
    const copyButton = must(shadow.querySelector<HTMLButtonElement>(".copy-button"), "the late copy button");
    longLineButton.click();
    copyButton.click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(longLineButton.closest(".dl-long")?.classList.contains("expanded")).toBe(true);
    expect(copyButton.dataset["state"]).toBe("ok");
    expect(copied).toBe("late chunk value");
    expect(host.getAttribute("aria-busy")).toBe("false");
    expect(host.dataset["diffComplete"]).toBe("true");
    expect(host.dataset["diffRowCount"]).toBe("2");
    island.destroy();
  });

  test("a replacement rejects the prior generation even when the semantic identity is unchanged", () => {
    installTrustedHtmlFixtures(
      new Map([
        [DOCUMENT_HTML, () => [diffDocument("src/lib.rs")]],
        [REPLACEMENT_HTML, () => [diffDocument("src/lib.rs")]],
        [CHUNK_HTML, () => [lateChunk()]],
      ]),
    );
    const host = document.createElement("section");
    installOpenShadowRoot(host);
    const island = createDiffIslandAdapter();
    const staleChain = island.mount(host, prepared("tab-1:range"));
    const activeChain = island.replace(prepared("tab-1:range", REPLACEMENT_HTML));
    const chunk = {
      viewIdentity: "tab-1:range",
      targetId: "viewer-diff-0",
      html: CHUNK_HTML,
      rowCount: 2,
      continuation: true,
    } as const;

    expect(island.appendChunk(staleChain, chunk)).toBe("stale");
    expect(island.appendChunk(activeChain, { ...chunk, viewIdentity: "tab-2:range" })).toBe("stale");
    expect(host.shadowRoot?.querySelector(".ln-more")).toBeNull();
    expect(host.getAttribute("aria-busy")).toBe("true");
    island.destroy();
  });

  test("rejects a network stylesheet before disturbing the mounted document", () => {
    installTrustedHtmlFixtures(new Map([[DOCUMENT_HTML, () => [diffDocument("src/lib.rs")]]]));
    const host = document.createElement("section");
    installOpenShadowRoot(host);
    const island = createDiffIslandAdapter();
    island.mount(host, prepared("tab-1:range"));
    const mountedRoot = host.shadowRoot?.querySelector(DIFF_DOCUMENT_SELECTOR);

    expect(() =>
      island.replace({
        ...prepared("tab-1:range"),
        styleHref: "https://example.invalid/diff.css",
      }),
    ).toThrow("app-local root-relative path");
    expect(host.shadowRoot?.querySelector(DIFF_DOCUMENT_SELECTOR)).toBe(mountedRoot);
    island.destroy();
  });

  test("restores an anchor only across matching identities and exposes Dioxus file actions", () => {
    installTrustedHtmlFixtures(
      new Map([
        [DOCUMENT_HTML, () => [diffDocument("src/lib.rs")]],
        [REPLACEMENT_HTML, () => [diffDocument("src/lib.rs")]],
      ]),
    );
    const host = document.createElement("section");
    installOpenShadowRoot(host);
    const island = createDiffIslandAdapter();
    island.mount(host, prepared("tab-1:range"));
    const beforeFile = must(host.shadowRoot?.querySelector<HTMLDetailsElement>("details.file") ?? null, "the file");
    const beforeRoot = must(
      host.shadowRoot?.querySelector<HTMLElement>("[data-gtl-diff-document]") ?? null,
      "the document root",
    );
    setTestRect(beforeRoot, { y: 40, height: 200 });
    setTestRect(beforeFile, { y: 54, height: 80 });

    island.replace(prepared("tab-1:range", REPLACEMENT_HTML));
    const matchingFile = must(
      host.shadowRoot?.querySelector<HTMLDetailsElement>("details.file") ?? null,
      "the matching replacement file",
    );
    expect(matchingFile.open).toBe(true);
    island.setFilesFolded(true);
    expect(matchingFile.open).toBe(false);
    expect(island.scrollToFile("src-lib-rs")).toBe(true);
    expect(matchingFile.open).toBe(true);

    island.replace(prepared("tab-2:range", DOCUMENT_HTML));
    expect(host.shadowRoot?.querySelector<HTMLDetailsElement>("details.file")?.open).toBe(false);
    island.destroy();
  });
});
