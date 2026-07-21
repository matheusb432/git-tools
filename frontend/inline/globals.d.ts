// Ambient types for the server-inlined scripts in this directory.
//
// The sibling .ts files ship VERBATIM as inline <script> content: the build
// copies their bytes into crates/*/src/embedded/generated/ (see the `build`
// task), so they must stay single-line, comment-free, and JS-syntax-only.
// Strong typing comes from `deno task typecheck` against these declarations,
// not from annotations in the shipped sources.

interface TauriEventApi {
  listen(event: string, handler: () => void): Promise<() => void>;
}

interface HtmxAjaxContext {
  target: string;
  swap: string;
}

interface HtmxApi {
  ajax(verb: string, path: string, context: HtmxAjaxContext): Promise<void>;
}

interface Window {
  __TAURI__: { event: TauriEventApi };
  htmx: HtmxApi;
}
