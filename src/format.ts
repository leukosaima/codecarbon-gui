/**
 * Human-friendly number formatting. Same rules (and test cases) as
 * src-tauri/src/format.rs, which formats the tray; keep both in sync.
 */
import type { Hardware, Location } from "./api";

const DASH = "–";

/** At most 3 significant digits, trailing zeros removed. Below 1e-6 → "0". */
export function sig3(x: number): string {
  if (!Number.isFinite(x)) return DASH;
  if (Math.abs(x) < 1e-6) return "0";
  const magnitude = Math.floor(Math.log10(Math.abs(x)));
  const decimals = Math.min(6, Math.max(0, 2 - magnitude));
  const s = x.toFixed(decimals);
  return s.includes(".") ? s.replace(/0+$/, "").replace(/\.$/, "") : s;
}

/** Watts with one decimal: "23.4 W". */
export function power(watts: number): string {
  if (!Number.isFinite(watts)) return DASH;
  return `${watts.toFixed(1)} W`;
}

/** Energy (input in kWh): "12.30 Wh" below 1000 Wh, then "1.20 kWh". */
export function energy(kwh: number): string {
  if (!Number.isFinite(kwh)) return DASH;
  const wh = kwh * 1000;
  return Math.abs(wh) < 999.995 ? `${wh.toFixed(2)} Wh` : `${kwh.toFixed(2)} kWh`;
}

/** CO₂e (input in kg): "0.84 g" (3 significant digits), then "1.2 kg". */
export function co2(kg: number): string {
  if (!Number.isFinite(kg)) return DASH;
  const g = kg * 1000;
  return Math.abs(g) < 999.5 ? `${sig3(g)} g` : `${sig3(kg)} kg`;
}

/** Duration as hh:mm:ss (hours may exceed 24). */
export function duration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "00:00:00";
  const s = Math.floor(seconds);
  const pad = (n: number): string => String(n).padStart(2, "0");
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
}

/** Grid carbon intensity as an integer: "56 g/kWh". */
export function intensity(gPerKwh: number): string {
  if (!Number.isFinite(gPerKwh)) return DASH;
  return `${gPerKwh.toFixed(0)} g/kWh`;
}

/** How CodeCarbon obtains the CPU power, in plain words. */
export function cpuMethod(hw: Hardware): string {
  const tdp = hw.cpu_tdp_w != null ? `TDP ${sig3(hw.cpu_tdp_w)} W` : null;
  switch (hw.cpu_method) {
    case "intel_rapl":
      return "measured (Intel RAPL)";
    case "windows_emi":
      return "measured (Windows EMI)";
    case "intel_power_gadget":
      return "measured (Intel Power Gadget)";
    case "apple_powermetrics":
      return "measured (powermetrics)";
    case "cpu_load":
      return tdp ? `estimated from load (${tdp})` : "estimated from load";
    case "constant":
      return tdp ? `constant estimate (${tdp})` : "constant estimate";
    case null:
    case undefined:
    case "":
      return "unknown";
    default:
      return hw.cpu_method;
  }
}

/**
 * One-line explanation of where the numbers come from, e.g.
 * "CPU: estimated from load (TDP 28 W) · GPU: NVML · France · 56 g/kWh".
 */
export function statusLine(
  hw: Hardware | null,
  loc: Location | null,
  intensityGPerKwh: number | null,
): string {
  const parts: string[] = [];
  if (hw) {
    parts.push(`CPU: ${cpuMethod(hw)}`);
    if (hw.gpu_method && (hw.gpu_count ?? 1) > 0) {
      parts.push(`GPU: ${hw.gpu_method === "nvml" ? "NVML" : hw.gpu_method}`);
    }
  }
  if (loc) {
    const place = [loc.country_name ?? loc.country_iso_code, loc.region].filter(Boolean).join(", ");
    if (place) parts.push(place);
  }
  const ci = intensityGPerKwh ?? loc?.carbon_intensity_g_per_kwh ?? null;
  if (ci != null) parts.push(intensity(ci));
  return parts.join(" · ");
}

/**
 * Parse a local "YYYY-MM-DD" or "YYYY-MM-DDTHH:MM:SS" (no offset, as in
 * emissions.csv) as *local* time. (`new Date("YYYY-MM-DD")` would be UTC.)
 */
export function parseLocal(iso: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(iso.trim());
  if (!m) return null;
  const parts = m.slice(1).map((x) => Number(x ?? 0));
  const [y = NaN, mo = NaN, d = NaN, hh = 0, mi = 0, ss = 0] = parts;
  const date = new Date(y, mo - 1, d, hh, mi, ss);
  // Reject roll-overs such as 2026-02-31.
  return Number.isNaN(date.getTime()) || date.getMonth() !== mo - 1 ? null : date;
}

/** `locale` undefined = the system locale. */
function dateFormat(
  locale: string | undefined,
  options: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  return new Intl.DateTimeFormat(locale, options);
}

/**
 * Date and time of a session: "Mon 2 Nov, 14:05" (en-GB). The year is
 * added when it is not the current one.
 */
export function formatDate(iso: string, locale?: string, now: Date = new Date()): string {
  const date = parseLocal(iso);
  if (!date) return DASH;
  return dateFormat(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

/** A day: "Mon 2 Nov" (en-GB). */
export function formatDay(date: string, locale?: string): string {
  const d = parseLocal(date);
  if (!d) return DASH;
  return dateFormat(locale, { weekday: "short", day: "numeric", month: "short" }).format(d);
}

/**
 * A chart tick: localised weekday, then the day of the month, in that
 * order whatever the locale ("Mon 2", "lun. 2"; en-US alone would say "2 Mon").
 */
export function formatTick(date: string, locale?: string): string {
  const d = parseLocal(date);
  if (!d) return DASH;
  const weekday = dateFormat(locale, { weekday: "short" }).format(d);
  const day = dateFormat(locale, { day: "numeric" }).format(d);
  return `${weekday} ${day}`;
}

/** A total duration in words: "45 s", "12 min", "3 h 5 min", "26 h". */
export function durationShort(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return DASH;
  if (seconds < 60) return `${Math.floor(seconds)} s`;
  const minutes = Math.round(seconds / 60);
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}
