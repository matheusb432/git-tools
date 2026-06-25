export function toggleLongLine(button: Element): void {
  const row = button.closest(".dl-long");
  if (!row) return;
  const on = row.classList.toggle("expanded");
  button.setAttribute("aria-expanded", on ? "true" : "false");
}
