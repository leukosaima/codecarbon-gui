import { describe, expect, it } from "vitest";
import { barChart, MIN_BAR } from "./barchart";

describe("barChart", () => {
  it("scales from 0 to the maximum, one slot per value", () => {
    const { bars, max } = barChart([0, 5, 10], 30, 100, 2);
    expect(max).toBe(10);
    expect(bars).toHaveLength(3);
    expect(bars[0]).toEqual({ x: 1, y: 100, width: 8, height: 0 });
    expect(bars[1]).toEqual({ x: 11, y: 50, width: 8, height: 50 });
    expect(bars[2]).toEqual({ x: 21, y: 0, width: 8, height: 100 });
  });

  it("keeps tiny days visible and ignores bad values", () => {
    const { bars, max } = barChart([1000, 0.001, Number.NaN, -4], 40, 100);
    expect(max).toBe(1000);
    expect(bars[1]?.height).toBe(MIN_BAR);
    expect(bars[2]?.height).toBe(0);
    expect(bars[3]?.height).toBe(0);
  });

  it("handles an empty or all-zero series", () => {
    expect(barChart([], 300, 100)).toEqual({ bars: [], max: 0 });
    const { bars, max } = barChart([0, 0], 300, 100);
    expect(max).toBe(0);
    expect(bars.every((b) => b.height === 0 && b.y === 100)).toBe(true);
  });
});
