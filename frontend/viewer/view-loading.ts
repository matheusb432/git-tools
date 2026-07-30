const LOADING_TEMPLATE_ID = "viewer-loading-template";

export function showViewerLoading(targetDocument: Document): boolean {
  const template = targetDocument.getElementById(LOADING_TEMPLATE_ID);
  const current = targetDocument.getElementById("viewer-view");
  if (!(template instanceof HTMLTemplateElement) || !(current instanceof HTMLElement)) return false;
  const replacement = template.content.firstElementChild?.cloneNode(true);
  if (!(replacement instanceof HTMLElement)) return false;
  current.className = replacement.className;
  current.setAttribute("data-viewer-state", "loading");
  current.setAttribute("aria-busy", "true");
  current.replaceChildren(...replacement.childNodes);
  targetDocument.querySelector(".viewer-tab.active")?.setAttribute("aria-busy", "true");
  return true;
}

function targetsViewerView(element: Element): boolean {
  return element.getAttribute("hx-target") === "#viewer-view";
}

function opensHistoryView(element: Element): boolean {
  const route = element.getAttribute("hx-get");
  return route !== null && route.startsWith("/history/") && route.endsWith("/open");
}

export function installViewerLoading(targetDocument: Document): void {
  if (targetDocument.getElementById("viewer-view")?.getAttribute("data-viewer-state") === "loading") {
    showViewerLoading(targetDocument);
  }
  targetDocument.addEventListener(
    "click",
    (event) => {
      const trigger = event.target instanceof Element ? event.target.closest("[hx-get],[hx-delete]") : null;
      if (!trigger) return;
      if (targetsViewerView(trigger) || opensHistoryView(trigger)) {
        const requestedView = targetDocument.getElementById("viewer-view");
        setTimeout(() => {
          if (targetDocument.getElementById("viewer-view") === requestedView) {
            showViewerLoading(targetDocument);
          }
        }, 0);
      }
    },
    { capture: true },
  );
}
