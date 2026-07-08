export function initTabs(): void {
  const tabs = Array.from(document.querySelectorAll<HTMLElement>(".tabs .tab"));
  if (!tabs.length) return;
  const panels = Array.from(document.querySelectorAll<HTMLElement>(".panel"));
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      const index = tab.getAttribute("data-tab");
      tabs.forEach((t) => {
        const active = t === tab;
        t.classList.toggle("active", active);
        t.setAttribute("aria-selected", String(active));
      });
      panels.forEach((panel) => {
        panel.hidden = panel.id !== `panel-${index}`;
      });
    });
  });
}
