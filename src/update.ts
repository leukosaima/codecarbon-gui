/**
 * Text of the update banner (Dashboard) and of the update line (About),
 * from `state.update` / `state.updater`. Pure, so it is unit-tested; the
 * views only put the result in the DOM.
 */
import type { Phase, UpdateInfo, UpdaterStatus } from "./api";
import { errorText, formatNumber, t } from "./i18n/i18n";

export interface UpdateBanner {
  /** "CodeCarbon 0.2.0 is available", "Downloading… 42 %", … */
  text: string;
  /** Second line: a warning or the last install error; null for none. */
  note: string | null;
  /** The note is an error (styled as such). */
  noteIsError: boolean;
  /** Show the Install button (only when nothing is in progress). */
  canInstall: boolean;
}

export interface UpdateInputs {
  update: UpdateInfo | null;
  updater: UpdaterStatus;
  /** The measurement phase: installing stops a running measurement. */
  phase: Phase;
}

/** The Dashboard banner, or null when there is nothing to show. */
export function updateBanner({ update, updater, phase }: UpdateInputs): UpdateBanner | null {
  if (!update || !updater.configured) return null;
  const version = update.version;
  switch (updater.phase) {
    case "downloading":
      return {
        text:
          updater.percent === null
            ? t("update.downloading_unknown", { version })
            : t("update.downloading", { version, percent: formatNumber(updater.percent, 0) }),
        note: null,
        noteIsError: false,
        canInstall: false,
      };
    case "installing":
      return {
        text: t("update.installing", { version }),
        note: null,
        noteIsError: false,
        canInstall: false,
      };
    default: {
      const failed = updater.error?.code === "update_install_failed";
      const measuring = phase === "starting" || phase === "running" || phase === "stopping";
      return {
        text: t("update.available", { version }),
        note: failed
          ? errorText(updater.error)
          : measuring
            ? t("update.stops_measurement")
            : null,
        noteIsError: failed,
        canInstall: updater.phase === "idle",
      };
    }
  }
}

/** About's line under "Check for updates" ("" for nothing to say yet). */
export function updateStatusLine({ update, updater }: Omit<UpdateInputs, "phase">): string {
  if (!updater.configured) return t("about.updates_not_configured");
  if (updater.phase === "checking") return t("about.checking");
  if (updater.error) return errorText(updater.error);
  if (update) return t("about.update_available", { version: update.version });
  return updater.checked ? t("about.up_to_date") : "";
}
