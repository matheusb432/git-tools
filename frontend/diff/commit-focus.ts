// * Returns true when the event target is (or is inside) the .sha copy button, so
// * the card's keydown/click handler can bail before re-focusing the card.
export function isShaTarget(target: { closest?: (selector: string) => unknown } | null): boolean {
  return !!(target && target.closest && target.closest(".sha"));
}

// ! Pure helper: compute the next (activeSha, activeSet) after a card click, so the
// ! toggle/merge logic can be unit-tested without a DOM. Returns null for both when toggling off.
export function resolveActiveSet(
  clickedSha: string,
  membersAttr: string,
  activeSha: string | null,
): { readonly sha: string | null; readonly set: string[] | null } {
  if (activeSha === clickedSha) return { sha: null, set: null };
  const members = membersAttr.split(" ").filter(Boolean);
  return { sha: clickedSha, set: members.length ? members : [clickedSha] };
}
