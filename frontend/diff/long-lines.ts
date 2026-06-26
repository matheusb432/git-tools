export function toggleLongLine(button: Element): void {
  // Unified tames at the row (.dl-long); side-by-side tames on the pane's own <code class="long">.
  const row = button.closest(".dl-long") ?? button.closest("code.long");
  if (!row) return;
  const on = row.classList.toggle("expanded");
  button.setAttribute("aria-expanded", on ? "true" : "false");
}
