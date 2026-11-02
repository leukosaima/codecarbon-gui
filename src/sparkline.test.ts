import { describe, expect, it } from "vitest";
import type { HistoryPoint } from "./api";
import { sparkline, WINDOW_SECS } from "./sparkline";

const pt = (t: number, w: number): HistoryPoint => ({
  ts: `t${t}`,
  duration_s: t,
  total_w: w,
  emissions_rate: 0,
});

describe("sparkline", () => {
  it("needs two points", () => {
    expect(sparkline([], 100, 50).line).toBe("");
    expect(sparkline([pt(0, 10)], 100, 50).line).toBe("");
  });

  it("spans the full width and autoscales", () => {
    const g = sparkline([pt(0, 10), pt(3, 20)], 100, 50);
    expect(g.max).toBe(20);
    // First point at x=0, last at x=100; 20 W is 10 % below the top.
    expect(g.line.startsWith("M0.0,")).toBe(true);
    expect(g.line).toContain("100.0,4.5");
    expect(g.area.endsWith("Z")).toBe(true);
  });

  it("keeps only the last 5 minutes", () => {
    const points = [pt(0, 500), pt(100, 10), pt(100 + WINDOW_SECS, 12)];
    const g = sparkline(points, 100, 50);
    expect(g.max).toBe(12); // the 500 W spike is outside the window
  });
});
