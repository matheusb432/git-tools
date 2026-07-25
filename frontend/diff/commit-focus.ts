/** The selected commit card and every sha it owns; absent together or present together. */
export type CommitFocus = {
  readonly sha: string;
  readonly shas: ReadonlySet<string>;
};

/** True when the event target sits inside the .sha copy button, so card handlers bail. */
export function isShaTarget(target: Element): boolean {
  return target.closest(".sha") !== null;
}

/** The next focus after a card click; null when the click toggles the current one off. */
export function resolveActiveSet(
  clickedSha: string,
  membersAttr: string,
  activeSha: string | null,
): CommitFocus | null {
  if (activeSha === clickedSha) return null;
  const members = membersAttr.split(" ").filter(Boolean);
  return { sha: clickedSha, shas: new Set(members.length ? members : [clickedSha]) };
}
