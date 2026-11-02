/**
 * Dashboard: Start/Stop, live tiles, power sparkline, how-it-is-measured
 * notes and the last session summary. Pure rendering of `TrackerState`.
 */
import {
  errorMessage,
  getEquivalents,
  type Equivalent,
  openDataFolder,
  restartAgent,
  startTracking,
  stopTracking,
  type Settings,
  type TrackerState,
} from "../api";
import { uploadChip } from "../account";
import { h, setText, show, spinner, svg } from "../dom";
import * as fmt from "../format";
import { cpuMethod, statusLine } from "../format";
import { best, describe as describeEquivalent } from "../equivalents";
import { live, liveAttr, n, t, tNodes, type MessageKey } from "../i18n/i18n";
import { sparkline, WINDOW_SECS } from "../sparkline";

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

  constructor() {
    this.banner.append(this.bannerText, this.retry);
    this.retry.addEventListener("click", () => void this.run(restartAgent));
    this.button.addEventListener("click", () => this.onButton());

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

    this.root = h(
      "section",
      { class: "view dashboard" },
      this.banner,
      this.loading,
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

  /** Saved settings (at load and after every save): the upload chip. */
  setSettings(settings: Settings): void {
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
    setText(this.power.value, power ? fmt.power(power.total) : "–");
    if (power) {
      // Components that are 0 (e.g. no GPU) are left out.
      const parts: [string, number][] = [
        ["CPU", power.cpu],
        ["GPU", power.gpu],
        ["RAM", power.ram],
      ];
      const breakdown = parts
        .filter(([name, w]) => w > 0 || name === "CPU")
        .map(([name, w]) => `${name} ${fmt.power(w)}`)
        .join(" · ");
      setText(this.power.sub, summary ? t("dashboard.average", { breakdown }) : breakdown);
    } else {
      setText(this.power.sub, "");
    }
    setText(this.co2.value, emissions !== undefined ? fmt.co2(emissions) : "–");
    setText(this.energy.value, energy !== undefined ? fmt.energy(energy) : "–");
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
    setText(this.status, line);
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
    const text = this.equivalent ? describeEquivalent(this.equivalent) : null;
    setText(this.summaryEquivalent, text ?? "");
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
