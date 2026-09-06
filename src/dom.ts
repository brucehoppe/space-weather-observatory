/** Tiny DOM helpers. Everything the UI renders goes through `el`/`text`, so
 *  provider text is inserted as text nodes and never as markup. */

type Attrs = Record<string, string | number | boolean | null | undefined>;
type Child = Node | string | null | undefined | false;

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "class") node.className = String(value);
    else if (key.startsWith("data-") || key.startsWith("aria-") || key === "role") {
      node.setAttribute(key, String(value));
    } else if (key in node) {
      (node as unknown as Record<string, unknown>)[key] = value;
    } else {
      node.setAttribute(key, String(value));
    }
  }
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
  return node;
}

export function on<K extends keyof HTMLElementEventMap>(
  node: HTMLElement,
  event: K,
  handler: (e: HTMLElementEventMap[K]) => void,
): HTMLElement {
  node.addEventListener(event, handler);
  return node;
}

export function button(label: string, onClick: () => void, className = "ghost"): HTMLButtonElement {
  const b = el("button", { class: className, type: "button" }, label);
  b.addEventListener("click", onClick);
  return b;
}

/** Append children, skipping absent ones. */
export function append(parent: HTMLElement, ...children: (Node | string | null | undefined | false)[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    parent.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
}

export function clear(node: HTMLElement): void {
  node.replaceChildren();
}

/** A single polite live region. Used sparingly: a newly active alert episode
 *  is announced once, and pointer movement is never announced (spec §13B). */
let liveRegion: HTMLElement | null = null;

export function announce(message: string): void {
  if (!liveRegion) {
    liveRegion = el("div", { class: "sr-only", role: "status", "aria-live": "polite" });
    document.body.append(liveRegion);
  }
  liveRegion.textContent = message;
}
