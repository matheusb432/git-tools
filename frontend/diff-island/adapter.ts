import {
  DIFF_DOCUMENT_SELECTOR,
  captureDiffDocumentAnchor,
  enhanceDiffDocument,
  restoreDiffDocumentAnchor,
  scrollDiffDocumentToFile,
  setDiffDocumentFilesFolded,
  type DiffDocumentAnchor,
} from "../diff/enhance-document";

export type DiffDocumentMaterialization = { readonly kind: "complete" } | { readonly kind: "loading" };

/** Rust-rendered markup inserted without client-side sanitization. */
export type ServerRenderedDiffHtml = string;

export type PreparedDiffDocument = {
  readonly viewIdentity: string;
  readonly html: ServerRenderedDiffHtml;
  readonly styleHref: string;
  readonly materialization: DiffDocumentMaterialization;
};

export type DiffChunk = {
  readonly viewIdentity: string;
  readonly targetId: string;
  readonly html: ServerRenderedDiffHtml;
  readonly rowCount: number;
  readonly continuation: boolean;
};

export type DiffIslandChain = {
  readonly viewIdentity: string;
  readonly generation: number;
};

export type AppendDiffChunkResult = "appended" | "complete" | "stale" | "target-missing";

export type DiffIslandAdapter = {
  readonly mount: (host: HTMLElement, prepared: PreparedDiffDocument) => DiffIslandChain;
  readonly replace: (prepared: PreparedDiffDocument) => DiffIslandChain;
  readonly appendChunk: (chain: DiffIslandChain, chunk: DiffChunk) => AppendDiffChunkResult;
  readonly scrollToFile: (targetId: string) => boolean;
  readonly setFilesFolded: (folded: boolean) => void;
  readonly destroy: () => void;
};

type PreparedNodes = {
  readonly documentRoot: HTMLElement;
  readonly stylesheet: HTMLLinkElement;
};

type MountedDiffDocument = {
  readonly host: HTMLElement;
  readonly shadowRoot: ShadowRoot;
  readonly documentRoot: HTMLElement;
  readonly cleanup: () => void;
  readonly viewIdentity: string;
  readonly generation: number;
  readonly complete: boolean;
  readonly rowCount: number;
};

function requirePreparedDocument(prepared: PreparedDiffDocument): void {
  if (!prepared.viewIdentity || typeof prepared.html !== "string") {
    throw new TypeError("diff document identity and HTML are required");
  }
  if (
    typeof prepared.styleHref !== "string" ||
    !prepared.styleHref.startsWith("/") ||
    prepared.styleHref.startsWith("//") ||
    prepared.styleHref.includes("\\")
  ) {
    throw new TypeError("diff document stylesheet must be an app-local root-relative path");
  }
  if (prepared.materialization.kind !== "complete" && prepared.materialization.kind !== "loading") {
    throw new TypeError("diff document materialization is invalid");
  }
}

function requireChunk(chain: DiffIslandChain, chunk: DiffChunk): void {
  if (
    !chain.viewIdentity ||
    !Number.isSafeInteger(chain.generation) ||
    chain.generation < 1 ||
    !chunk.viewIdentity ||
    !chunk.targetId ||
    typeof chunk.html !== "string" ||
    !Number.isSafeInteger(chunk.rowCount) ||
    chunk.rowCount < 0 ||
    typeof chunk.continuation !== "boolean"
  ) {
    throw new TypeError("diff chunk is invalid");
  }
}

function openShadowRoot(host: HTMLElement): ShadowRoot {
  return host.shadowRoot ?? host.attachShadow({ mode: "open" });
}

function prepareNodes(prepared: PreparedDiffDocument): PreparedNodes {
  requirePreparedDocument(prepared);
  const template = document.createElement("template");
  template.innerHTML = prepared.html;
  const documentRoots = template.content.querySelectorAll<HTMLElement>(DIFF_DOCUMENT_SELECTOR);
  if (documentRoots.length !== 1) {
    throw new TypeError(`diff document HTML contained ${documentRoots.length} semantic roots`);
  }
  const documentRoot = documentRoots[0];
  if (!documentRoot) throw new TypeError("diff document root is missing");
  const stylesheet = document.createElement("link");
  stylesheet.rel = "stylesheet";
  stylesheet.href = prepared.styleHref;
  return { documentRoot, stylesheet };
}

function trustedChunkNodes(html: string): readonly Node[] {
  const template = document.createElement("template");
  template.innerHTML = html;
  return [...template.content.childNodes];
}

function setCompletionState(state: MountedDiffDocument): void {
  const complete = state.complete ? "true" : "false";
  state.host.dataset["viewIdentity"] = state.viewIdentity;
  state.host.dataset["diffComplete"] = complete;
  state.host.dataset["diffRowCount"] = String(state.rowCount);
  state.host.setAttribute("aria-busy", state.complete ? "false" : "true");
  state.documentRoot.dataset["diffComplete"] = complete;
}

function clearHostState(host: HTMLElement): void {
  delete host.dataset["viewIdentity"];
  delete host.dataset["diffComplete"];
  delete host.dataset["diffRowCount"];
  host.removeAttribute("aria-busy");
}

export function createDiffIslandAdapter(): DiffIslandAdapter {
  let generation = 0;
  let mounted: MountedDiffDocument | null = null;

  const nextGeneration = (): number => {
    if (generation >= Number.MAX_SAFE_INTEGER) throw new RangeError("diff island generation is exhausted");
    generation += 1;
    return generation;
  };

  const replaceMounted = (
    host: HTMLElement,
    shadowRoot: ShadowRoot,
    prepared: PreparedDiffDocument,
    previous: MountedDiffDocument | null,
  ): DiffIslandChain => {
    const nodes = prepareNodes(prepared);
    const cleanup = enhanceDiffDocument(nodes.documentRoot);
    const anchor: DiffDocumentAnchor | null = previous ? captureDiffDocumentAnchor(previous.documentRoot) : null;
    const restoreAnchor = previous?.viewIdentity === prepared.viewIdentity;
    const nextState: MountedDiffDocument = {
      host,
      shadowRoot,
      documentRoot: nodes.documentRoot,
      cleanup,
      viewIdentity: prepared.viewIdentity,
      generation: nextGeneration(),
      complete: prepared.materialization.kind === "complete",
      rowCount: 0,
    };

    previous?.cleanup();
    shadowRoot.replaceChildren(nodes.stylesheet, nodes.documentRoot);
    mounted = nextState;
    setCompletionState(nextState);
    if (restoreAnchor && anchor) restoreDiffDocumentAnchor(nodes.documentRoot, anchor);
    return { viewIdentity: nextState.viewIdentity, generation: nextState.generation };
  };

  const destroy = (): void => {
    const current = mounted;
    if (!current) return;
    mounted = null;
    current.cleanup();
    current.shadowRoot.replaceChildren();
    clearHostState(current.host);
  };

  return {
    mount(host, prepared) {
      if (!(host instanceof HTMLElement)) throw new TypeError("diff island host must be an HTML element");
      const shadowRoot = openShadowRoot(host);
      const previous = mounted;
      if (previous && previous.host !== host) {
        const nodes = prepareNodes(prepared);
        const cleanup = enhanceDiffDocument(nodes.documentRoot);
        const nextState: MountedDiffDocument = {
          host,
          shadowRoot,
          documentRoot: nodes.documentRoot,
          cleanup,
          viewIdentity: prepared.viewIdentity,
          generation: nextGeneration(),
          complete: prepared.materialization.kind === "complete",
          rowCount: 0,
        };
        destroy();
        shadowRoot.replaceChildren(nodes.stylesheet, nodes.documentRoot);
        mounted = nextState;
        setCompletionState(nextState);
        return { viewIdentity: nextState.viewIdentity, generation: nextState.generation };
      }
      return replaceMounted(host, shadowRoot, prepared, previous);
    },
    replace(prepared) {
      if (!mounted) throw new Error("diff island is not mounted");
      return replaceMounted(mounted.host, mounted.shadowRoot, prepared, mounted);
    },
    appendChunk(chain, chunk) {
      requireChunk(chain, chunk);
      const current = mounted;
      if (
        !current ||
        current.generation !== chain.generation ||
        current.viewIdentity !== chain.viewIdentity ||
        current.viewIdentity !== chunk.viewIdentity
      ) {
        return "stale";
      }
      if (current.complete) return "complete";
      const target = current.shadowRoot.getElementById(chunk.targetId);
      if (!(target instanceof HTMLElement) || !target.matches(".diff") || !current.documentRoot.contains(target)) {
        return "target-missing";
      }
      const rowCount = current.rowCount + chunk.rowCount;
      if (!Number.isSafeInteger(rowCount)) throw new RangeError("diff chunk row count is exhausted");
      const nodes = trustedChunkNodes(chunk.html);
      nodes.forEach((node) => target.appendChild(node));
      const nextState: MountedDiffDocument = {
        ...current,
        complete: !chunk.continuation,
        rowCount,
      };
      mounted = nextState;
      setCompletionState(nextState);
      return "appended";
    },
    scrollToFile(targetId) {
      if (typeof targetId !== "string" || !targetId) throw new TypeError("diff file target ID is required");
      return mounted ? scrollDiffDocumentToFile(mounted.documentRoot, targetId) : false;
    },
    setFilesFolded(folded) {
      if (typeof folded !== "boolean") throw new TypeError("diff file folded state must be a boolean");
      if (mounted) setDiffDocumentFilesFolded(mounted.documentRoot, folded);
    },
    destroy,
  };
}
