/**
 * Everyday equivalents in words ("≈ 12 km by car"). The conversion factors
 * live in Rust only (`history::FACTORS`); this module just phrases the raw
 * numbers it returns, with the same number formatting as the rest of the app.
 */
import type { Equivalent } from "./api";
import { sig3 } from "./format";

/** "1 hour" / "2.5 hours", then minutes and seconds below an hour. */
function timeAmount(hours: number): string {
  if (hours >= 1) return `${sig3(hours)} ${sig3(hours) === "1" ? "hour" : "hours"}`;
  const minutes = hours * 60;
  if (minutes >= 1) return `${sig3(minutes)} min`;
  return `${sig3(minutes * 60)} s`;
}

/** The sentence for one equivalent, or null for an unknown key. */
export function describe(eq: Equivalent): string | null {
  const v = Number.isFinite(eq.value) ? Math.max(0, eq.value) : 0;
  switch (eq.key) {
    case "car_km":
      return v >= 1 ? `≈ ${sig3(v)} km by car` : `≈ ${sig3(v * 1000)} m by car`;
    case "smartphone_charges":
      if (v >= 1) return `≈ ${sig3(v)} smartphone ${sig3(v) === "1" ? "charge" : "charges"}`;
      return `≈ ${sig3(v * 100)} % of a smartphone charge`;
    case "led_tv_hours":
      return `≈ ${timeAmount(v)} of LED TV`;
    case "streaming_hours":
      return `≈ ${timeAmount(v)} of video streaming`;
    default:
      return null;
  }
}

/**
 * The one equivalent to show for a single session: the first (in the
 * order Rust lists them) that is at least one whole unit, otherwise the
 * largest one. Null when there is nothing to compare (no emissions).
 */
export function best(list: Equivalent[]): Equivalent | null {
  const known = list.filter((e) => describe(e) !== null && Number.isFinite(e.value) && e.value > 0);
  if (known.length === 0) return null;
  return known.find((e) => e.value >= 1) ?? known.reduce((a, b) => (b.value > a.value ? b : a));
}

/** 20×20 stroke icons (inline SVG paths, `currentColor`). */
export const ICONS: Record<string, string> = {
  // A car seen from the side.
  car_km:
    "M3 13.5V11l2-4.5h10l2 4.5v2.5h-1.5M3 13.5h1.5m3 0h5M5 11h10 M6 15.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Zm8 0a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z",
  // A phone with a charge bolt.
  smartphone_charges:
    "M6.5 2.5h7a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-13a1 1 0 0 1 1-1Z M10.8 6.5 8.8 10h2.4l-2 3.5",
  // A screen on a stand.
  led_tv_hours: "M2.5 4.5h15v9h-15z M7 17h6 M10 13.5V17",
  // A play button.
  streaming_hours: "M10 17.5a7.5 7.5 0 1 0 0-15 7.5 7.5 0 0 0 0 15Z M8.5 7v6l5-3z",
};
