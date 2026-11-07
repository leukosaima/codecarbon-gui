/**
 * Brand pictures of the CodeCarbon design system (Figma "Code Carbon -
 * Master"): the logo and the lime pixel icons, bundled by Vite.
 */
import { h, svg } from "./dom";
import { ICONS } from "./equivalents";
import logo from "./assets/brand/codecarbon-logo.svg";
import house from "./assets/brand/pixel-house.svg";
import truck from "./assets/brand/pixel-truck.svg";
import tv from "./assets/brand/pixel-tv.svg";

export const LOGO_URL = logo;
export const HOUSE_URL = house;

/** Equivalents drawn with a brand pixel icon (the others keep a stroke icon). */
const PIXEL_ICONS: Record<string, string> = {
  car_km: truck, // the design's "kilometers riden"
  led_tv_hours: tv,
};

/** The icon of an equivalent, decorative (its sentence says it all). */
export function equivalentIcon(key: string): HTMLImageElement | SVGSVGElement {
  const pixel = PIXEL_ICONS[key];
  if (pixel) {
    // Sized by its square slot in styles.css; it keeps its own aspect ratio.
    return h("img", { class: "eq-icon", src: pixel, alt: "", "aria-hidden": "true" });
  }
  const el = svg("svg", { class: "eq-icon", viewBox: "0 0 20 20", "aria-hidden": "true" });
  el.append(svg("path", { d: ICONS[key] ?? "" }));
  return el;
}
