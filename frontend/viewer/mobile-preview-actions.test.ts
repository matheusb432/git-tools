import { describe, expect, test, vi } from "vitest";
import { installMobilePreviewActions } from "./mobile-preview-actions";

describe("installMobilePreviewActions", () => {
  test("mobile view actions delegate to the active server-rendered layout", () => {
    const view = document.createElement("section");
    view.id = "viewer-view";
    const layout = document.createElement("div");
    layout.className = "layout";
    const foldAll = document.createElement("button");
    foldAll.className = "foldall";
    const toggleContext = document.createElement("button");
    toggleContext.className = "ctx-toggle";
    const refresh = document.createElement("button");
    refresh.setAttribute("hx-get", "/tabs/1/refresh");
    const deleteLive = document.createElement("button");
    deleteLive.className = "viewer-danger-button";
    const controls = document.createElement("header");
    controls.className = "viewer-controls";
    controls.appendChild(refresh);
    controls.appendChild(deleteLive);
    layout.appendChild(foldAll);
    layout.appendChild(toggleContext);
    view.appendChild(layout);
    view.appendChild(controls);
    const foldProxy = document.createElement("button");
    foldProxy.setAttribute("data-mobile-preview-action", "fold-all");
    const contextProxy = document.createElement("button");
    contextProxy.setAttribute("data-mobile-preview-action", "toggle-context");
    const refreshProxy = document.createElement("button");
    refreshProxy.setAttribute("data-mobile-preview-action", "refresh");
    const deleteProxy = document.createElement("button");
    deleteProxy.setAttribute("data-mobile-preview-action", "delete-live-view");
    view.appendChild(foldProxy);
    view.appendChild(contextProxy);
    view.appendChild(refreshProxy);
    view.appendChild(deleteProxy);
    document.body.appendChild(view);
    const foldClick = vi.fn();
    const contextClick = vi.fn();
    const refreshClick = vi.fn();
    const deleteClick = vi.fn();
    foldAll.addEventListener("click", foldClick);
    toggleContext.addEventListener("click", contextClick);
    refresh.addEventListener("click", refreshClick);
    deleteLive.addEventListener("click", deleteClick);

    installMobilePreviewActions(document);
    foldProxy.click();
    contextProxy.click();
    refreshProxy.click();
    deleteProxy.click();

    expect(foldClick).toHaveBeenCalledOnce();
    expect(contextClick).toHaveBeenCalledOnce();
    expect(refreshClick).toHaveBeenCalledOnce();
    expect(deleteClick).toHaveBeenCalledOnce();
  });
});
