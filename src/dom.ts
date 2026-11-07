/** Tiny DOM helpers (no framework). Text is always set with textContent. */
import { splitUnit } from "./format";

type Child = Node | string | null | undefined | false;
type Attrs = Record<string, string | number | boolean | null | undefined>;

/** Create an element: h("div", { class: "tile" }, "text", otherNode). */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === false || value === null || value === undefined) continue;
    el.setAttribute(name, value === true ? "" : String(value));
  }
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    el.append(child);
  }
  return el;
}

/** Create an SVG element. */
export function svg<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
): SVGElementTagNameMap[K] {
  const el = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === false || value === null || value === undefined) continue;
    el.setAttribute(name, String(value));
  }
  return el;
}

/** Set text only when it changed (avoids needless layout work). */
export function setText(el: Element, text: string): void {
  if (el.textContent !== text) el.textContent = text;
}

/**
 * A KPI ("23.4 W"): the number as text, the unit in a `.tile-unit` span
 * (styled apart). The element's text stays the whole quantity.
 */
export function setQuantity(el: Element, text: string): void {
  if (el.textContent === text) return;
  const [value, unit] = splitUnit(text);
  el.replaceChildren(value);
  if (unit) el.append(h("span", { class: "tile-unit" }, unit));
}

/** Show or hide an element with the `hidden` attribute. */
export function show(el: HTMLElement, visible: boolean): void {
  if (el.hidden === visible) el.hidden = !visible;
}

/** A small inline spinner. */
export function spinner(): HTMLSpanElement {
  return h("span", { class: "spinner", "aria-hidden": "true" });
}
