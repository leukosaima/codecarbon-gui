import { describe, expect, it } from "vitest";
import { showWelcome } from "./welcome";

const firstRun = { dismissed: false, hasLastSummary: false, hasSessions: false };

describe("first-run welcome card", () => {
  it("shows on a first run", () => {
    expect(showWelcome(firstRun)).toBe(true);
  });

  it("hides once dismissed", () => {
    expect(showWelcome({ ...firstRun, dismissed: true })).toBe(false);
  });

  it("hides once a session stopped, in this run or before", () => {
    expect(showWelcome({ ...firstRun, hasLastSummary: true })).toBe(false);
    expect(showWelcome({ ...firstRun, hasSessions: true })).toBe(false);
  });

  it("stays hidden while the settings or the history are unknown", () => {
    expect(showWelcome({ ...firstRun, dismissed: null })).toBe(false);
    expect(showWelcome({ ...firstRun, hasSessions: null })).toBe(false);
  });
});
