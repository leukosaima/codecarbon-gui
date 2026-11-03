import { afterEach, describe, expect, it } from "vitest";
import type { UpdaterStatus } from "./api";
import { setLocale } from "./i18n/i18n";
import { updateBanner, updateStatusLine } from "./update";

const idle: UpdaterStatus = {
  configured: true,
  phase: "idle",
  checked: false,
  percent: null,
  error: null,
};
const update = { version: "0.2.0", date: "2026-11-01T10:00:00Z", notes: null };

afterEach(() => {
  setLocale("en", []);
});

describe("update banner", () => {
  it("is hidden without an update, or in a build without updater", () => {
    expect(updateBanner({ update: null, updater: idle, phase: "idle" })).toBeNull();
    expect(
      updateBanner({ update, updater: { ...idle, configured: false }, phase: "idle" }),
    ).toBeNull();
  });

  it("offers to install", () => {
    expect(updateBanner({ update, updater: idle, phase: "idle" })).toEqual({
      text: "CodeCarbon 0.2.0 is available",
      note: null,
      noteIsError: false,
      canInstall: true,
    });
  });

  it("warns that a running measurement is stopped first", () => {
    const banner = updateBanner({ update, updater: idle, phase: "running" });
    expect(banner?.note).toBe("The current measurement will be stopped and saved first.");
    expect(banner?.canInstall).toBe(true);
  });

  it("shows the download progress, without an Install button", () => {
    const downloading: UpdaterStatus = { ...idle, phase: "downloading", percent: 42 };
    expect(updateBanner({ update, updater: downloading, phase: "idle" })).toMatchObject({
      text: "Downloading CodeCarbon 0.2.0… 42 %",
      canInstall: false,
    });
    expect(
      updateBanner({ update, updater: { ...downloading, percent: null }, phase: "idle" })?.text,
    ).toBe("Downloading CodeCarbon 0.2.0…");
    setLocale("fr", []);
    expect(updateBanner({ update, updater: downloading, phase: "idle" })?.text).toBe(
      "Téléchargement de CodeCarbon 0.2.0… 42 %",
    );
  });

  it("says the app restarts while installing", () => {
    const banner = updateBanner({ update, updater: { ...idle, phase: "installing" }, phase: "idle" });
    expect(banner).toMatchObject({
      text: "Installing CodeCarbon 0.2.0… The app will restart.",
      canInstall: false,
    });
  });

  it("shows an install error and lets the user try again", () => {
    const failed: UpdaterStatus = {
      ...idle,
      error: { code: "update_install_failed", detail: "signature", message: "x" },
    };
    expect(updateBanner({ update, updater: failed, phase: "running" })).toEqual({
      text: "CodeCarbon 0.2.0 is available",
      note: "The update could not be installed (signature).",
      noteIsError: true,
      canInstall: true,
    });
  });

  it("does not show a check error (About does)", () => {
    const failed: UpdaterStatus = {
      ...idle,
      error: { code: "update_check_failed", detail: "offline", message: "x" },
    };
    expect(updateBanner({ update, updater: failed, phase: "idle" })?.note).toBeNull();
  });

  it("has no Install button while a check is running", () => {
    const checking: UpdaterStatus = { ...idle, phase: "checking" };
    expect(updateBanner({ update, updater: checking, phase: "idle" })?.canInstall).toBe(false);
  });
});

describe("About update line", () => {
  it("follows the updater", () => {
    expect(updateStatusLine({ update: null, updater: { ...idle, configured: false } })).toBe(
      "Automatic updates are not configured for this build.",
    );
    expect(updateStatusLine({ update: null, updater: idle })).toBe("");
    expect(updateStatusLine({ update: null, updater: { ...idle, phase: "checking" } })).toBe(
      "Checking for updates…",
    );
    expect(updateStatusLine({ update: null, updater: { ...idle, checked: true } })).toBe(
      "You are up to date.",
    );
    expect(updateStatusLine({ update, updater: { ...idle, checked: true } })).toBe(
      "Version 0.2.0 available.",
    );
    const failed: UpdaterStatus = {
      ...idle,
      checked: true,
      error: { code: "update_check_failed", detail: "offline", message: "x" },
    };
    expect(updateStatusLine({ update: null, updater: failed })).toBe(
      "Update check failed: offline",
    );
  });

  it("is translated", () => {
    setLocale("fr", []);
    expect(updateStatusLine({ update: null, updater: { ...idle, checked: true } })).toBe(
      "Vous avez la dernière version.",
    );
    expect(updateStatusLine({ update, updater: idle })).toBe("Version 0.2.0 disponible.");
  });
});
