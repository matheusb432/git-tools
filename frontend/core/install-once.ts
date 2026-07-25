/**
 * Wraps a document-level installer so each document is wired at most once. Every wrapper keeps
 * its own record, so wiring one feature never blocks another on the same document.
 */
export function installOnce<Args extends readonly unknown[]>(
  install: (root: Document, ...args: Args) => void,
): (root: Document, ...args: Args) => void {
  const installedDocuments = new WeakSet<Document>();
  return (root, ...args) => {
    if (installedDocuments.has(root)) return;
    installedDocuments.add(root);
    install(root, ...args);
  };
}
