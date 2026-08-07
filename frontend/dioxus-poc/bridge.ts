import { createUpdateCoordinator, parseDiffFragmentState } from "./bridge-model";

const HOST_ID = "dioxus-diff-island";
const PENDING_EVENT = "recipes-pending";
const COMPLETED_EVENT = "recipe-completed";
const PROCESS_PENDING_COMMAND = "dioxus_poc_process_pending";
const DIFF_FRAGMENT_COMMAND = "dioxus_poc_diff_fragment";
const STOP_KEY = "__GTL_DIOXUS_POC_BRIDGE_STOP__";

type Invoke = (command: string) => Promise<unknown>;
type Unlisten = () => void;
type Listen = (event: string, handler: () => void) => Promise<Unlisten>;
type Runtime = { readonly invoke: Invoke; readonly listen: Listen };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function invokes(value: unknown): value is Invoke {
  return typeof value === "function";
}

function listens(value: unknown): value is Listen {
  return typeof value === "function";
}

function readRuntime(value: unknown): Runtime | null {
  if (!isRecord(value) || !isRecord(value.__TAURI__)) return null;
  const { core, event } = value.__TAURI__;
  if (!isRecord(core) || !isRecord(event)) return null;
  const invoke = core.invoke;
  const listen = event.listen;
  if (!invokes(invoke) || !listens(listen)) return null;
  return {
    invoke: (command) => invoke(command),
    listen: (name, handler) => listen(name, handler),
  };
}

function islandHost(): HTMLElement | null {
  const host = document.getElementById(HOST_ID);
  return host instanceof HTMLElement ? host : null;
}

function shadowRoot(host: HTMLElement): ShadowRoot {
  return host.shadowRoot ?? host.attachShadow({ mode: "open" });
}

function renderMessage(host: HTMLElement, message: string, state: "connecting" | "error"): void {
  const style = document.createElement("style");
  style.textContent =
    ":host{display:grid;min-height:0;height:100%;place-items:center;background:#090d13;color:#9aa7b5;font:500 13px ui-monospace,monospace}.message{max-width:34rem;padding:2rem;text-align:center}";
  const content = document.createElement("div");
  content.className = "message";
  content.setAttribute("role", state === "error" ? "alert" : "status");
  content.textContent = message;
  shadowRoot(host).replaceChildren(style, content);
  host.dataset.bridgeState = state;
  host.setAttribute("aria-busy", state === "connecting" ? "true" : "false");
}

function applyFragment(host: HTMLElement, html: string, css: string): void {
  const template = document.createElement("template");
  template.innerHTML = html;
  const views = template.content.querySelectorAll("#viewer-view");
  if (views.length !== 1 || !(views[0] instanceof HTMLElement)) {
    throw new Error(`backend response contained ${views.length} viewer roots`);
  }

  const style = document.createElement("style");
  style.textContent = css;
  const frame = document.createElement("div");
  frame.append(views[0]);
  shadowRoot(host).replaceChildren(style, frame);
  host.dataset.bridgeState = "ready";
  host.setAttribute("aria-busy", "false");
}

function installBridge(runtime: Runtime): () => void {
  let activeHost: HTMLElement | null = null;
  let hostObserver: MutationObserver | null = null;
  let initialized = false;
  let stopped = false;
  const unlisten: Unlisten[] = [];

  const requestUpdate = createUpdateCoordinator(
    async (update) => {
      if (update === "pending-recipes") {
        activeHost?.setAttribute("aria-busy", "true");
        await runtime.invoke(PROCESS_PENDING_COMMAND);
        return;
      }

      const state = parseDiffFragmentState(await runtime.invoke(DIFF_FRAGMENT_COMMAND));
      if (!state) throw new Error("backend returned an invalid diff fragment payload");
      if (!activeHost) return;
      if (state.state === "pending") {
        activeHost.dataset.bridgeState = "pending";
        activeHost.setAttribute("aria-busy", "true");
        return;
      }
      applyFragment(activeHost, state.fragment.html, state.fragment.css);
    },
    (update, error) => {
      console.error(`Dioxus proof bridge failed during ${update}`, error);
      if (activeHost) renderMessage(activeHost, "The Rust-rendered diff could not be loaded.", "error");
    },
  );

  const mount = (): void => {
    const nextHost = islandHost();
    if (nextHost === activeHost) return;
    hostObserver?.disconnect();
    activeHost = nextHost;
    if (!activeHost) return;
    renderMessage(activeHost, "Connecting to the Rust diff renderer…", "connecting");
    hostObserver = new MutationObserver((records) => {
      if (records.some((record) => record.attributeName === "data-refresh-revision")) {
        void requestUpdate("ready-fragment");
      }
    });
    hostObserver.observe(activeHost, { attributes: true, attributeFilter: ["data-refresh-revision"] });
    if (initialized) void requestUpdate("ready-fragment");
  };

  const documentObserver = new MutationObserver(mount);
  documentObserver.observe(document.documentElement, { childList: true, subtree: true });
  mount();

  void Promise.all([
    runtime.listen(PENDING_EVENT, () => void requestUpdate("pending-recipes")),
    runtime.listen(COMPLETED_EVENT, () => void requestUpdate("ready-fragment")),
  ])
    .then((subscriptions) => {
      if (stopped) subscriptions.forEach((stop) => stop());
      else unlisten.push(...subscriptions);
    })
    .then(async () => {
      if (stopped) return;
      initialized = true;
      await requestUpdate("pending-recipes");
      await requestUpdate("ready-fragment");
    })
    .catch((error: unknown) => {
      console.error("Dioxus proof bridge could not subscribe to Tauri events", error);
      if (activeHost) renderMessage(activeHost, "The Tauri bridge is unavailable.", "error");
    });

  return () => {
    stopped = true;
    documentObserver.disconnect();
    hostObserver?.disconnect();
    unlisten.splice(0).forEach((stop) => stop());
  };
}

const previousStop = Reflect.get(globalThis, STOP_KEY);
if (typeof previousStop === "function") previousStop();
const runtime = readRuntime(globalThis);
const stop = runtime
  ? installBridge(runtime)
  : () => {
      const host = islandHost();
      if (host) renderMessage(host, "This proof must run inside its Tauri development shell.", "error");
    };
Reflect.set(globalThis, STOP_KEY, stop);
