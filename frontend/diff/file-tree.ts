export type FileLeaf = {
  readonly name: string;
  readonly status: string;
  readonly statusCode: string;
  readonly statusLabel: string;
  readonly el: { readonly id: string };
};

// * The Rust-rendered tree root owns presentation for these semantic hooks.
export function buildFileLeaf(doc: Document, file: FileLeaf): HTMLElement {
  const li = doc.createElement("li");
  li.className = `tnode tfile status-${file.status}`;
  li.setAttribute("data-target", file.el.id);
  const label = doc.createElement("div");
  label.className = "tlabel";
  const name = doc.createElement("span");
  name.className = "tname";
  name.textContent = file.name;
  const status = doc.createElement("span");
  status.className = `tstatus status-${file.status}`;
  status.textContent = file.statusCode;
  status.title = file.statusLabel;
  status.setAttribute("aria-label", file.statusLabel);
  label.appendChild(name);
  label.appendChild(status);
  li.appendChild(label);
  return li;
}
