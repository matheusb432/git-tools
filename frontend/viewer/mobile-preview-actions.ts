import { installOnce } from "../shared/install-once";

export const installMobilePreviewActions = installOnce((root: Document): void => {
  root.addEventListener("click", (event) => {
    if (!(event.target instanceof Element)) return;
    const trigger = event.target.closest<HTMLElement>("[data-mobile-preview-action]");
    if (!trigger) return;
    const view = root.getElementById("viewer-view");
    const layout = view?.querySelector<HTMLElement>(".layout");
    if (!layout) return;

    switch (trigger.getAttribute("data-mobile-preview-action")) {
      case "fold-all":
        layout.querySelector<HTMLButtonElement>(".foldall")?.click();
        return;
      case "toggle-context":
        layout.querySelector<HTMLButtonElement>(".ctx-toggle")?.click();
        return;
      case "refresh":
        view?.querySelector<HTMLButtonElement>(".viewer-controls [hx-get*='/refresh']")?.click();
        return;
      case "delete-live-view":
        view?.querySelector<HTMLButtonElement>(".viewer-danger-button")?.click();
        return;
      default:
        return;
    }
  });
});
