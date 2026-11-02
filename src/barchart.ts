/** Pure geometry for the History bar chart (unit-tested, no DOM). */

export interface Bar {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface BarChartGeometry {
  bars: Bar[];
  /** Largest value (the top of the chart). */
  max: number;
}

/** Bars of a non-zero day are at least this tall, so they stay visible. */
export const MIN_BAR = 1.5;

/**
 * One bar per value in a `width` × `height` box (y grows downwards, bars
 * stand on y = height), separated by `gap`. Scaled from 0 to the maximum.
 */
export function barChart(
  values: number[],
  width: number,
  height: number,
  gap = 2,
): BarChartGeometry {
  const clean = values.map((v) => (Number.isFinite(v) && v > 0 ? v : 0));
  const max = Math.max(0, ...clean);
  const slot = clean.length > 0 ? width / clean.length : 0;
  const barWidth = Math.max(slot - gap, 1);
  const bars = clean.map((v, i) => {
    const h = max > 0 && v > 0 ? Math.max((v / max) * height, MIN_BAR) : 0;
    return { x: i * slot + (slot - barWidth) / 2, y: height - h, width: barWidth, height: h };
  });
  return { bars, max };
}
