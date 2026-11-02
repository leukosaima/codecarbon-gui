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
