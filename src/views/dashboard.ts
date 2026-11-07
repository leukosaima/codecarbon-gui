/**
 * Dashboard: first-run welcome card, Start/Stop, live tiles, power
 * sparkline, how-it-is-measured notes and the last session summary.
 * Pure rendering of `TrackerState`.
 */
import {
  errorMessage,
  getEquivalents,
  getFirstRunInfo,
  installUpdate,
  type Equivalent,
  openDataFolder,
  restartAgent,
  saveSettings,
  startTracking,
  stopTracking,
  type Settings,
  type TrackerState,
} from "../api";
import { uploadChip } from "../account";
import { equivalentIcon } from "../brand";
import { h, setQuantity, setText, show, spinner, svg } from "../dom";
import * as fmt from "../format";
import { cpuMethod, statusLine } from "../format";
import { best, describe as describeEquivalent, numberParts } from "../equivalents";
import { getLocale, live, liveAttr, n, t, tNodes, type Locale, type MessageKey } from "../i18n/i18n";
import { sparkline, WINDOW_SECS } from "../sparkline";
import { updateBanner } from "../update";
import { showWelcome } from "../welcome";

const CHART_W = 300;
const CHART_H = 64;

/** A big-number tile. */
function tile(label: MessageKey): { root: HTMLElement; value: HTMLElement; sub: HTMLElement } {
  const value = h("div", { class: "tile-value" }, "–");
  const sub = h("div", { class: "tile-sub" });
  const labelEl = h("div", { class: "tile-label" }, live(label));
  const root = h("div", { class: "tile" }, labelEl, value, sub);
  return { root, value, sub };
}

export class DashboardView {
  readonly root: HTMLElement;

  private state: TrackerState | null = null;
  /** When the latest measurement arrived, to tick the duration every second. */
  private latestReceivedAt = 0;
  private latestTs: string | null = null;

  private readonly banner = h("div", { class: "banner", role: "alert" });
  private readonly bannerText = h("span");
  private readonly retry = h(
    "button",
    { class: "btn btn-small", type: "button" },
    live("dashboard.retry"),
  );
  private readonly loading = h("div", { class: "hint" }, spinner(), live("common.starting_engine"));
  // "CodeCarbon 0.2.0 is available — Install" (see update.ts).
  private readonly updateText = h("span", { class: "update-text" });
  private readonly updateNote = h("small", { class: "update-note" });
  private readonly updateInstall = h(
    "button",
    { class: "btn btn-small btn-primary", type: "button" },
    live("update.install"),
  );
  private readonly updateCard = h(
    "div",
    { class: "update-banner", role: "status" },
    h("div", { class: "update-body" }, this.updateText, this.updateNote),
    this.updateInstall,
  );
  private readonly button = h("button", { class: "btn btn-primary btn-big", type: "button" });
  private readonly power = tile("dashboard.power");
  private readonly co2 = tile("dashboard.co2");
  private readonly energy = tile("dashboard.energy");
  private readonly duration = tile("dashboard.duration");
  private readonly chartCard: HTMLElement;
  private readonly chartLine = svg("path", { class: "spark-line" });
  private readonly chartArea = svg("path", { class: "spark-area" });
  private readonly chartMax = h("span", { class: "muted" });
  private readonly status = h("p", { class: "status-line" });
  private readonly uploadChip = h("p", { class: "upload-chip" });
  private readonly summary: HTMLElement;
  private readonly summaryRows = h("dl", { class: "summary-grid" });
  /** "≈ 0.4 km by car" for the last session. */
  private readonly summaryEquivalent = h("p", { class: "summary-equivalent" });
  /** Run id whose equivalent is shown (or being fetched). */
  private equivalentFor: string | null = null;
  /** The equivalent shown, phrased again when the language changes. */
  private equivalent: Equivalent | null = null;
  private readonly details = h("details", { class: "card details" });
  private readonly detailsBody = h("div", { class: "details-body" });
  /** The power breakdown shown, to rebuild its parts only when it changes. */
  private breakdownKey = "";

  // First-run welcome card (see welcome.ts for when it shows).
  private readonly welcome: HTMLElement;
  /** "Press **Start measuring** to…", built again when the language changes. */
  private readonly welcomeStart = h("p");
  private welcomeStartLocale: Locale | null = null;
  private readonly welcomeGnome = h("p", { class: "muted small" }, live("welcome.gnome"));
  private settings: Settings | null = null;
  /** From `get_first_run_info`; null until it answered. */
  private hasSessions: boolean | null = null;

  constructor() {
    this.banner.append(this.bannerText, this.retry);
    this.retry.addEventListener("click", () => void this.run(restartAgent));
    this.button.addEventListener("click", () => this.onButton());
    this.updateInstall.addEventListener("click", () => {
      this.updateInstall.disabled = true; // until the next state (downloading)
      void this.run(installUpdate);
    });
    show(this.updateCard, false);

    const minutes = WINDOW_SECS / 60;
    const chart = liveAttr(
      svg("svg", {
        class: "spark",
        viewBox: `0 0 ${CHART_W} ${CHART_H}`,
        preserveAspectRatio: "none",
        role: "img",
      }),
      "aria-label",
      "dashboard.chart_label",
      { minutes },
    );
    chart.append(this.chartArea, this.chartLine);
    this.chartCard = h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("span", {}, live("dashboard.chart_title", { minutes })),
        this.chartMax,
      ),
      chart,
    );

    const openFolder = h(
      "button",
      { class: "link", type: "button" },
      live("common.open_data_folder"),
    );
    openFolder.addEventListener("click", () => void this.run(openDataFolder));
    this.summary = h(
      "section",
      { class: "card summary" },
      h(
        "div",
        { class: "card-head" },
        h("span", {}, live("dashboard.last_session")),
        openFolder,
      ),
      this.summaryRows,
      this.summaryEquivalent,
    );
    show(this.summaryEquivalent, false);

    this.details.append(h("summary", {}, live("dashboard.how_measured")), this.detailsBody);

    const dismiss = h(
      "button",
      { class: "btn btn-small btn-primary", type: "button" },
      live("welcome.dismiss"),
    );
    dismiss.addEventListener("click", () => void this.dismissWelcome());
    this.welcome = h(
      "section",
      { class: "card welcome", "aria-labelledby": "welcome-title" },
      h("h2", { class: "welcome-title", id: "welcome-title" }, live("welcome.title")),
      h("p", {}, live("welcome.tray")),
      this.welcomeStart,
      h("p", {}, live("welcome.privacy")),
      this.welcomeGnome,
      h("div", { class: "welcome-actions" }, dismiss),
    );
    show(this.welcome, false);
    show(this.welcomeGnome, false);
    this.power.sub.classList.add("breakdown");

    this.root = h(
      "section",
      { class: "view dashboard" },
      this.banner,
      this.updateCard,
      this.loading,
      this.welcome,
      this.button,
      this.summary,
      h(
        "div",
        { class: "tiles" },
        this.power.root,
        this.co2.root,
        this.energy.root,
        this.duration.root,
      ),
      this.chartCard,
      this.status,
      this.uploadChip,
      this.details,
    );

    // Tick the duration tile between two measurements.
    window.setInterval(() => this.renderDuration(), 1000);
    show(this.uploadChip, false);

    getFirstRunInfo()
      .then((info) => {
        this.hasSessions = info.has_sessions;
        // Without the AppIndicator extension GNOME shows no tray icon at all.
        show(this.welcomeGnome, info.platform === "linux");
        this.renderWelcome();
      })
      .catch((e: unknown) => console.warn("get_first_run_info failed", e));
  }

  /** Show or hide the welcome card; its text follows the language. */
  private renderWelcome(): void {
    const visible = showWelcome({
      dismissed: this.settings?.welcome_dismissed ?? null,
      hasLastSummary: this.state?.last_summary != null,
      hasSessions: this.hasSessions,
    });
    show(this.welcome, visible);
    if (visible && this.welcomeStartLocale !== getLocale()) {
      this.welcomeStartLocale = getLocale();
      this.welcomeStart.replaceChildren(
        ...tNodes("welcome.start", { start: h("b", {}, t("dashboard.start")) }),
      );
    }
  }

  /** "Got it": hidden at once, remembered in the settings. */
  private async dismissWelcome(): Promise<void> {
    const settings = this.settings;
    if (!settings) return;
    show(this.welcome, false);
    try {
      // `settings://changed` then calls setSettings with the saved copy.
      this.setSettings(await saveSettings({ ...settings, welcome_dismissed: true }));
    } catch (e) {
      this.showError(errorMessage(e));
      this.renderWelcome();
    }
  }

  private async run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
    } catch (e) {
      this.showError(errorMessage(e));
    }
  }

  private showError(message: string): void {
    setText(this.bannerText, message);
    show(this.retry, false);
    show(this.banner, true);
  }

  private onButton(): void {
    const phase = this.state?.phase;
    void this.run(phase === "running" ? stopTracking : startTracking);
  }

  /** Saved settings (at load and after every save): upload chip, welcome card. */
  setSettings(settings: Settings): void {
    this.settings = settings;
    this.renderWelcome();
    const target = uploadChip(settings.api);
    this.uploadChip.replaceChildren();
    if (target) {
      this.uploadChip.append(
        h("span", { "aria-hidden": "true" }, "☁"),
        " ",
        ...tNodes("dashboard.uploading_to", { target: h("b", {}, target) }),
      );
      this.uploadChip.title = t("dashboard.uploading_title");
    }
    show(this.uploadChip, target !== null);
  }

  update(state: TrackerState): void {
    this.state = state;
    const { phase } = state;
    const measuring = phase === "running" || phase === "stopping";

    // Error banner + loading hint.
    show(this.banner, state.error !== null);
    setText(this.bannerText, state.error ? errorMessage(state.error) : "");
    show(this.retry, phase === "agent_error");
    show(this.loading, state.agent_ready === null && phase !== "agent_error");
    this.renderUpdate(state);

    // Start/Stop button.
    this.button.replaceChildren();
    this.button.classList.toggle("btn-stop", phase === "running");
    switch (phase) {
      case "starting":
        this.button.append(spinner(), ` ${t("dashboard.starting")}`);
        break;
      case "stopping":
        this.button.append(spinner(), ` ${t("dashboard.stopping")}`);
        break;
      case "running":
        this.button.append(t("dashboard.stop"));
        break;
      default:
        this.button.append(t("dashboard.start"));
    }
    this.button.disabled = !(phase === "running" || (phase === "idle" && state.agent_ready));

    // Tiles: live values, or the last session's values once stopped.
    const m = state.latest;
    const summary = phase === "idle" ? state.last_summary : null;
    // Live values while measuring; the final figures of the last session
    // once stopped (so the tiles agree with the summary card).
    const power = summary ? summary.avg_power_w : m?.power_w;
    const emissions = summary ? summary.emissions_kg : m?.emissions_kg;
    const energy = summary ? summary.energy_kwh.total : m?.energy_kwh.total;
    const rate = summary ? summary.emissions_rate_kg_s : m?.emissions_rate_kg_s;
    setQuantity(this.power.value, power ? fmt.power(power.total) : "–");
    if (power) {
      // Components that are 0 (e.g. no GPU) are left out.
      const parts: [string, number][] = [
        ["CPU", power.cpu],
        ["GPU", power.gpu],
        ["RAM", power.ram],
      ];
      const breakdown = parts
        .filter(([name, w]) => w > 0 || name === "CPU")
        .map(([name, w]) => `${name} ${fmt.power(w)}`);
      this.renderBreakdown(summary ? [t("dashboard.average"), ...breakdown] : breakdown);
    } else {
      this.renderBreakdown([]);
    }
    setQuantity(this.co2.value, emissions !== undefined ? fmt.co2(emissions) : "–");
    setQuantity(this.energy.value, energy !== undefined ? fmt.energy(energy) : "–");
    setText(this.co2.sub, rate ? t("dashboard.per_hour", { value: fmt.co2(rate * 3600) }) : "");
    const ci =
      summary?.carbon_intensity_g_per_kwh ??
      m?.carbon_intensity_g_per_kwh ??
      state.location?.carbon_intensity_g_per_kwh ??
      null;
    setText(this.energy.sub, ci != null ? t("dashboard.grid", { value: fmt.intensity(ci) }) : "");
    for (const box of [this.power, this.co2, this.energy, this.duration]) {
      box.root.classList.toggle("tile-live", measuring);
    }
    if (m && m.ts !== this.latestTs) {
      this.latestTs = m.ts;
      this.latestReceivedAt = performance.now();
    }
    this.renderDuration();

    // Sparkline.
    const geometry = sparkline(state.history, CHART_W, CHART_H);
    this.chartLine.setAttribute("d", geometry.line);
    this.chartArea.setAttribute("d", geometry.area);
    setText(
      this.chartMax,
      geometry.line ? t("dashboard.chart_max", { value: fmt.power(geometry.max) }) : "",
    );
    show(this.chartCard, state.history.length > 1);

    // Status line.
    const line = statusLine(state.hardware, state.location, m?.carbon_intensity_g_per_kwh ?? null);
    if (this.status.textContent !== line) {
      // One unbreakable part per item: the line wraps between them only.
      this.status.replaceChildren(
        ...line
          .split(" · ")
          .flatMap((part, i) => [i > 0 ? " · " : "", h("span", { class: "status-part" }, part)]),
      );
    }
    show(this.status, line !== "");

    // Last session summary.
    show(this.summary, summary !== null);
    if (summary) {
      const rows: [string, string][] = [
        [t("dashboard.duration"), fmt.duration(summary.duration_s)],
        [t("dashboard.energy"), fmt.energy(summary.energy_kwh.total)],
        [t("dashboard.co2"), fmt.co2(summary.emissions_kg)],
        [t("dashboard.average_power"), fmt.power(summary.avg_power_w.total)],
      ];
      if (summary.carbon_intensity_g_per_kwh != null) {
        rows.push([
          t("dashboard.grid_intensity"),
          fmt.intensity(summary.carbon_intensity_g_per_kwh),
        ]);
      }
      this.summaryRows.replaceChildren(
        ...rows.flatMap(([k, v]) => [h("dt", {}, k), h("dd", {}, v)]),
      );
      this.renderEquivalent(summary.run_id, summary.emissions_kg, summary.energy_kwh.total);
    }

    this.renderDetails(state);
    this.renderWelcome();
  }

  /** The update banner: available → Install → progress → restart. */
  private renderUpdate(state: TrackerState): void {
    const banner = updateBanner(state);
    show(this.updateCard, banner !== null);
    if (!banner) return;
    setText(this.updateText, banner.text);
    setText(this.updateNote, banner.note ?? "");
    show(this.updateNote, banner.note !== null);
    this.updateNote.classList.toggle("error", banner.noteIsError);
    show(this.updateInstall, banner.canInstall);
    this.updateInstall.disabled = false;
  }

  /**
   * "CPU 1.6 W · RAM 10.0 W", one element per part: on a narrow panel the
   * parts wrap onto a second line instead of being cut off (styles.css
   * draws the "·" between parts on the same line).
   */
  private renderBreakdown(parts: string[]): void {
    const key = parts.join("\n");
    if (key === this.breakdownKey) return;
    this.breakdownKey = key;
    this.power.sub.replaceChildren(...parts.map((part) => h("span", {}, part)));
  }

  /** One everyday equivalent of the session (factors from Rust). */
  private renderEquivalent(runId: string, emissionsKg: number, energyKwh: number): void {
    if (this.equivalentFor === runId) {
      this.showEquivalent(); // same session; maybe another language
      return;
    }
    this.equivalentFor = runId;
    this.equivalent = null;
    show(this.summaryEquivalent, false);
    getEquivalents(emissionsKg, energyKwh)
      .then((list) => {
        if (this.equivalentFor !== runId) return;
        this.equivalent = best(list);
        this.showEquivalent();
      })
      .catch((e: unknown) => console.warn("get_equivalents failed", e));
  }

  private showEquivalent(): void {
    const eq = this.equivalent;
    const text = eq ? describeEquivalent(eq) : null;
    if (eq && text !== null && this.summaryEquivalent.textContent !== text) {
      const [before, value, after] = numberParts(text);
      this.summaryEquivalent.replaceChildren(
        equivalentIcon(eq.key),
        h("span", {}, before, h("span", { class: "eq-value" }, value), after),
      );
    }
    show(this.summaryEquivalent, text !== null);
  }

  private renderDuration(): void {
    const s = this.state;
    const m = s?.latest;
    if (!s || !m) {
      setText(this.duration.value, fmt.duration(0));
      setText(this.duration.sub, "");
      return;
    }
    let seconds = m.duration_s;
    if (s.phase === "running") {
      // Interpolate between measurements, but never more than 10 s ahead.
      seconds += Math.min((performance.now() - this.latestReceivedAt) / 1000, 10);
    } else if (s.phase === "idle" && s.last_summary) {
      seconds = s.last_summary.duration_s;
    }
    setText(this.duration.value, fmt.duration(seconds));
    const util = m.cpu_utilization_percent;
    setText(
      this.duration.sub,
      util != null ? t("dashboard.cpu_load", { value: fmt.decimal(util, 0) }) : "",
    );
  }

  private renderDetails(state: TrackerState): void {
    const hw = state.hardware;
    const items: HTMLElement[] = [];
    if (hw) {
      const threads = hw.cpu_count ? n("details.threads", hw.cpu_count) : null;
      const cpu = [hw.cpu_model, threads].filter(Boolean).join(", ");
      items.push(
        h(
          "li",
          {},
          h("b", {}, t("details.cpu")),
          `${cpu || t("details.unknown_model")} — ${cpuMethod(hw)}`,
        ),
      );
      if (hw.cpu_tdp_is_generic) {
        items.push(h("li", {}, t("details.generic_tdp")));
      }
      if (hw.gpu_count && hw.gpu_count > 0) {
        items.push(
          h(
            "li",
            {},
            h("b", {}, t("details.gpu")),
            `${hw.gpu_model ?? t("details.unknown")} — ${hw.gpu_method ?? "?"}`,
          ),
        );
      }
      if (hw.ram_total_gb != null) {
        items.push(
          h(
            "li",
            {},
            h("b", {}, t("details.ram")),
            t("details.ram_value", {
              value: fmt.decimal(hw.ram_total_gb, 1),
              method: hw.ram_method ?? t("details.ram_estimate"),
            }),
          ),
        );
      }
    }
    const loc = state.location;
    if (loc) {
      const where = [loc.country_name ?? loc.country_iso_code, loc.region]
        .filter(Boolean)
        .join(", ");
      const ci = loc.carbon_intensity_g_per_kwh;
      items.push(
        h(
          "li",
          {},
          h("b", {}, t("details.electricity")),
          `${where || t("details.unknown_location")}${ci != null ? ` — ${fmt.intensity(ci)}` : ""}`,
        ),
      );
    }
    // CodeCarbon's own warnings are shown as it wrote them (English).
    const notes = state.notes.map((note) => h("li", { class: "note", lang: "en" }, note));
    const children: HTMLElement[] = [
      h("p", {}, t("details.intro")),
      items.length
        ? h("ul", {}, ...items)
        : h("p", { class: "muted" }, t("details.no_hardware")),
    ];
    if (notes.length) {
      children.push(h("p", { class: "muted" }, t("details.notes")));
      children.push(h("ul", { class: "notes" }, ...notes));
    }
    this.detailsBody.replaceChildren(...children);
  }
}
