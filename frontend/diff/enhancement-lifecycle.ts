export type EnhancementLifecycle = {
  readonly enhanceWithin: (scope: ParentNode) => void;
  readonly destroyWithin: (scope: ParentNode) => void;
};

function rootsWithin(scope: ParentNode, selector: string): readonly HTMLElement[] {
  if (scope instanceof HTMLElement && scope.matches(selector)) return [scope];
  return [...scope.querySelectorAll<HTMLElement>(selector)];
}

export function createEnhancementLifecycle(
  rootSelector: string,
  mountRoot: (root: HTMLElement) => () => void,
): EnhancementLifecycle {
  const mounted = new WeakMap<HTMLElement, () => void>();
  return {
    enhanceWithin(scope) {
      rootsWithin(scope, rootSelector).forEach((root) => {
        if (mounted.has(root)) return;
        mounted.set(root, mountRoot(root));
        root.dataset["gtlEnhanced"] = "true";
      });
    },
    destroyWithin(scope) {
      rootsWithin(scope, rootSelector).forEach((root) => {
        const unmount = mounted.get(root);
        if (!unmount) return;
        unmount();
        mounted.delete(root);
        delete root.dataset["gtlEnhanced"];
      });
    },
  };
}
