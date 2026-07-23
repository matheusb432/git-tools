// Minimal DOM for frontend tests. It models the tree, selector, event, and geometry behavior
// needed to exercise progressive enhancement without a browser dependency.
if (typeof globalThis.document === "undefined") {
  const rectangles = new WeakMap<object, DOMRect>();

  function rectangle(init: DOMRectInit): DOMRect {
    const x = init.x ?? 0;
    const y = init.y ?? 0;
    const width = init.width ?? 0;
    const height = init.height ?? 0;
    return {
      x,
      y,
      width,
      height,
      top: y,
      right: x + width,
      bottom: y + height,
      left: x,
      toJSON: () => ({ x, y, width, height }),
    };
  }

  class StubClassList {
    readonly #tokens = new Set<string>();

    set value(value: string) {
      this.#tokens.clear();
      value
        .split(/\s+/)
        .filter(Boolean)
        .forEach((token) => this.#tokens.add(token));
    }

    get value(): string {
      return [...this.#tokens].join(" ");
    }

    add(...tokens: string[]): void {
      tokens.forEach((token) => this.#tokens.add(token));
    }

    remove(...tokens: string[]): void {
      tokens.forEach((token) => this.#tokens.delete(token));
    }

    contains(token: string): boolean {
      return this.#tokens.has(token);
    }

    toggle(token: string, force?: boolean): boolean {
      const enabled = force ?? !this.#tokens.has(token);
      if (enabled) this.#tokens.add(token);
      else this.#tokens.delete(token);
      return enabled;
    }
  }

  class StubNode extends EventTarget {
    readonly #listeners = new Map<string, Set<EventListenerOrEventListenerObject>>();
    parentNode: StubNode | null = null;
    childNodes: StubNode[] = [];
    nodeType = 1;
    textContent: string | null = "";

    get parentElement(): StubElement | null {
      return this.parentNode instanceof StubElement ? this.parentNode : null;
    }

    get firstChild(): StubNode | null {
      return this.childNodes[0] ?? null;
    }

    appendChild<T extends StubNode>(child: T): T {
      child.remove();
      child.parentNode = this;
      this.childNodes.push(child);
      return child;
    }

    insertBefore<T extends StubNode>(child: T, before: StubNode | null): T {
      child.remove();
      child.parentNode = this;
      const index = before === null ? -1 : this.childNodes.indexOf(before);
      if (index === -1) this.childNodes.push(child);
      else this.childNodes.splice(index, 0, child);
      return child;
    }

    removeChild<T extends StubNode>(child: T): T {
      const index = this.childNodes.indexOf(child);
      if (index !== -1) this.childNodes.splice(index, 1);
      child.parentNode = null;
      return child;
    }

    remove(): void {
      this.parentNode?.removeChild(this);
    }

    replaceChildren(...children: StubNode[]): void {
      this.childNodes.forEach((child) => {
        child.parentNode = null;
      });
      this.childNodes = [];
      children.forEach((child) => this.appendChild(child));
    }

    contains(node: StubNode | null): boolean {
      let current = node;
      while (current) {
        if (current === this) return true;
        current = current.parentNode;
      }
      return false;
    }

    override addEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      _options?: boolean | AddEventListenerOptions,
    ): void {
      if (!listener) return;
      const listeners = this.#listeners.get(type) ?? new Set<EventListenerOrEventListenerObject>();
      listeners.add(listener);
      this.#listeners.set(type, listeners);
    }

    override removeEventListener(
      type: string,
      listener: EventListenerOrEventListenerObject | null,
      _options?: boolean | EventListenerOptions,
    ): void {
      if (listener) this.#listeners.get(type)?.delete(listener);
    }

    override dispatchEvent(event: Event): boolean {
      if (event.target === null) Object.defineProperty(event, "target", { configurable: true, value: this });
      Object.defineProperty(event, "currentTarget", { configurable: true, value: this });
      this.#listeners.get(event.type)?.forEach((listener) => {
        if (typeof listener === "function") listener.call(this, event);
        else listener.handleEvent(event);
      });
      if (event.bubbles && !event.cancelBubble) this.parentNode?.dispatchEvent(event);
      return !event.defaultPrevented;
    }
  }

  function parseSelector(selector: string): {
    readonly tag?: string;
    readonly id?: string;
    readonly classes: readonly string[];
    readonly attribute?: { readonly name: string; readonly value?: string };
  } {
    const attributeMatch = selector.match(/\[([^=\]]+)(?:="([^"]*)")?\]/);
    const withoutAttribute = selector.replace(/\[[^\]]+\]/g, "");
    const idMatch = withoutAttribute.match(/#([\w-]+)/);
    const classes = [...withoutAttribute.matchAll(/\.([\w-]+)/g)].map((match) => match[1]).filter((value) => value !== undefined);
    const tagMatch = withoutAttribute.match(/^[\w-]+/);
    const attributeName = attributeMatch?.[1];
    return {
      ...(tagMatch?.[0] ? { tag: tagMatch[0].toUpperCase() } : {}),
      ...(idMatch?.[1] ? { id: idMatch[1] } : {}),
      classes,
      ...(attributeName
        ? {
            attribute: {
              name: attributeName,
              ...(attributeMatch?.[2] !== undefined ? { value: attributeMatch[2] } : {}),
            },
          }
        : {}),
    };
  }

  class StubElement extends StubNode {
    readonly tagName: string;
    readonly classList = new StubClassList();
    readonly dataset: Record<string, string> = {};
    readonly style: Record<string, string> = {};
    readonly #attributes = new Map<string, string>();
    id = "";
    hidden = false;
    open = false;
    scrollTop = 0;
    scrollLeft = 0;
    scrollHeight = 0;
    scrollWidth = 0;
    clientHeight = 0;
    clientWidth = 0;
    offsetHeight = 0;
    offsetWidth = 0;
    value = "";
    #innerHTML = "";

    constructor(tagName: string) {
      super();
      this.tagName = tagName.toUpperCase();
    }

    set className(value: string) {
      this.classList.value = value;
    }

    get className(): string {
      return this.classList.value;
    }

    set innerHTML(value: string) {
      this.#innerHTML = value;
      this.replaceChildren();
    }

    get innerHTML(): string {
      return this.#innerHTML;
    }

    setAttribute(name: string, value: string): void {
      this.#attributes.set(name, value);
      if (name === "id") this.id = value;
      else if (name === "class") this.className = value;
      else if (name.startsWith("data-")) {
        const key = name
          .slice(5)
          .replace(/-([a-z])/g, (_match, letter: string) => letter.toUpperCase());
        this.dataset[key] = value;
      }
    }

    getAttribute(name: string): string | null {
      if (name === "id") return this.id || null;
      if (name === "class") return this.className || null;
      return this.#attributes.get(name) ?? null;
    }

    removeAttribute(name: string): void {
      this.#attributes.delete(name);
    }

    #matchesSimple(selector: string): boolean {
      const parsed = parseSelector(selector);
      if (parsed.tag && parsed.tag !== this.tagName) return false;
      if (parsed.id && parsed.id !== this.id) return false;
      if (parsed.classes.some((className) => !this.classList.contains(className))) return false;
      if (!parsed.attribute) return true;
      const actual = this.getAttribute(parsed.attribute.name);
      return actual !== null && (parsed.attribute.value === undefined || actual === parsed.attribute.value);
    }

    matches(selector: string): boolean {
      return selector.split(",").some((part) => {
        const segments = part.trim().split(/\s+/);
        const ownSelector = segments.pop();
        if (!ownSelector || !this.#matchesSimple(ownSelector)) return false;
        let ancestor = this.parentElement;
        while (segments.length > 0) {
          const ancestorSelector = segments.pop();
          if (!ancestorSelector) return false;
          while (ancestor && !ancestor.#matchesSimple(ancestorSelector)) ancestor = ancestor.parentElement;
          if (!ancestor) return false;
          ancestor = ancestor.parentElement;
        }
        return true;
      });
    }

    querySelectorAll(selector: string): StubElement[] {
      const matches: StubElement[] = [];
      const visit = (node: StubNode): void => {
        node.childNodes.forEach((child) => {
          if (child instanceof StubElement && child.matches(selector)) matches.push(child);
          visit(child);
        });
      };
      visit(this);
      return matches;
    }

    querySelector(selector: string): StubElement | null {
      return this.querySelectorAll(selector)[0] ?? null;
    }

    closest(selector: string): StubElement | null {
      let current: StubElement | null = this;
      while (current) {
        if (current.matches(selector)) return current;
        current = current.parentElement;
      }
      return null;
    }

    getBoundingClientRect(): DOMRect {
      return rectangles.get(this) ?? rectangle({});
    }

    click(): void {
      this.dispatchEvent(new Event("click", { bubbles: true }));
    }

    focus(): void {}
    blur(): void {}
    select(): void {}
  }

  class StubComment extends StubNode {
    override nodeType = 8;
  }

  class StubDocument extends StubNode {
    readonly documentElement = new StubElement("html");
    readonly body = new StubElement("body");

    constructor() {
      super();
      this.appendChild(this.documentElement);
      this.documentElement.appendChild(this.body);
    }

    createElement(tagName: string): StubElement {
      return new StubElement(tagName);
    }

    createComment(): StubComment {
      return new StubComment();
    }

    createTreeWalker(): { nextNode(): null } {
      return { nextNode: () => null };
    }

    querySelectorAll(selector: string): StubElement[] {
      return this.documentElement.matches(selector)
        ? [this.documentElement, ...this.documentElement.querySelectorAll(selector)]
        : this.documentElement.querySelectorAll(selector);
    }

    querySelector(selector: string): StubElement | null {
      return this.querySelectorAll(selector)[0] ?? null;
    }

    getElementById(id: string): StubElement | null {
      return this.querySelector(`#${id}`);
    }

    execCommand(): boolean {
      return false;
    }
  }

  Object.defineProperties(globalThis, {
    Node: { value: StubNode },
    Element: { value: StubElement },
    HTMLElement: { value: StubElement },
    HTMLDetailsElement: { value: StubElement },
    HTMLInputElement: { value: StubElement },
    document: { value: new StubDocument(), writable: true },
  });

  Object.defineProperty(globalThis, "setTestRect", {
    value: (element: object, rect: DOMRectInit): void => {
      rectangles.set(element, rectangle(rect));
    },
  });
}

if (typeof globalThis.window === "undefined") {
  Object.defineProperty(globalThis, "window", {
    value: Object.assign(globalThis, { CSS: undefined, getSelection: () => null, innerHeight: 0 }),
    writable: true,
  });
}
if (typeof globalThis.navigator === "undefined") {
  Object.defineProperty(globalThis, "navigator", { value: {}, writable: true });
}
if (typeof globalThis.requestAnimationFrame === "undefined") {
  Object.defineProperty(globalThis, "requestAnimationFrame", {
    value: (callback: FrameRequestCallback): number => {
      callback(0);
      return 0;
    },
  });
}

declare global {
  function setTestRect(element: object, rect: DOMRectInit): void;
}

export {};
