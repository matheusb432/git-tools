// Host globals of the viewer document: Tauri injects __TAURI__ and the inlined
// htmx bootstrap assigns window.htmx before the viewer bundle runs. Both stay
// optional so every use site proves presence before calling through.

interface TauriEventApi {
  listen(event: string, handler: () => void): Promise<() => void>;
}

interface HtmxAjaxContext {
  readonly target: string;
  readonly swap: string;
}

interface HtmxApi {
  ajax(verb: string, path: string, context: HtmxAjaxContext): Promise<void>;
}

interface Window {
  __TAURI__?: { event: TauriEventApi };
  htmx?: HtmxApi;
}
