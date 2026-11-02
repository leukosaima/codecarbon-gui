/** Pure geometry for the power sparkline (unit-tested, no DOM). */
import type { HistoryPoint } from "./api";

/** Only the last 5 minutes are drawn. */
export const WINDOW_SECS = 300;

export interface SparklineGeometry {
  /** SVG path of the line ("" when there is nothing to draw). */
  line: string;
  /** Same path closed down to the baseline, for the filled area. */
  area: string;
  /** Highest value of the window (the top of the chart is 10 % above). */
  max: number;
}

/**
 * Compute the sparkline paths for `points` in a `width` × `height` box.
 * x = time (the last WINDOW_SECS seconds), y = total power, autoscaled from 0.
 */
export function sparkline(points: HistoryPoint[], width: number, height: number): SparklineGeometry {
  const last = points[points.length - 1];
  if (!last || points.length < 2) return { line: "", area: "", max: last?.total_w ?? 0 };
  const t1 = last.duration_s;
  const visible = points.filter((p) => p.duration_s >= t1 - WINDOW_SECS);
  const t0 = Math.max(t1 - WINDOW_SECS, visible[0]?.duration_s ?? t1);
  const span = Math.max(t1 - t0, 1e-9);
  const max = Math.max(...visible.map((p) => p.total_w), 0);
  const top = max > 0 ? max * 1.1 : 1;
  const coords = visible.map((p) => {
    const x = ((p.duration_s - t0) / span) * width;
    const y = height - (Math.max(p.total_w, 0) / top) * height;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  const line = `M${coords.join(" L")}`;
  const first = coords[0]?.split(",")[0] ?? "0";
  const lastX = coords[coords.length - 1]?.split(",")[0] ?? String(width);
  const area = `${line} L${lastX},${height} L${first},${height} Z`;
  return { line, area, max };
}
