/** True when the event target sits inside the .sha copy button, so card handlers bail. */
export function isShaTarget(target: { closest?: (selector: string) => unknown } | null): boolean {
  return !!(target && target.closest && target.closest(".sha"));
}

/** Next (activeSha, activeSet) after a card click; both null when the click toggles off. */
export function resolveActiveSet(
  clickedSha: string,
  membersAttr: string,
  activeSha: string | null,
): { readonly sha: string | null; readonly set: string[] | null } {
  if (activeSha === clickedSha) return { sha: null, set: null };
  const members = membersAttr.split(" ").filter(Boolean);
  return { sha: clickedSha, set: members.length ? members : [clickedSha] };
}
