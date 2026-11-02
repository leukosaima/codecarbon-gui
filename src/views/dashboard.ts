/**
 * Dashboard: Start/Stop, live tiles, power sparkline, how-it-is-measured
 * notes and the last session summary. Pure rendering of `TrackerState`.
 */
import {
  errorMessage,
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
import { sparkline, WINDOW_SECS } from "../sparkline";

const CHART_W = 300;
const CHART_H = 64;

/** A big-number tile. */
function tile(label: string): { root: HTMLElement; value: HTMLElement; sub: HTMLElement } {
  const value = h("div", { class: "tile-value" }, "–");
  const sub = h("div", { class: "tile-sub" });
  const root = h("div", { class: "tile" }, h("div", { class: "tile-label" }, label), value, sub);
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
  private readonly retry = h("button", { class: "btn btn-small", type: "button" }, "Retry");
  private readonly loading = h(
    "div",
    { class: "hint" },
    spinner(),
    " Starting the measurement engine…",
  );
  private readonly button = h("button", { class: "btn btn-primary btn-big", type: "button" });
  private readonly power = tile("Power");
  private readonly co2 = tile("CO₂e");
  private readonly energy = tile("Energy");
  private readonly duration = tile("Duration");
  private readonly chartCard: HTMLElement;
  private readonly chartLine = svg("path", { class: "spark-line" });
  private readonly chartArea = svg("path", { class: "spark-area" });
  private readonly chartMax = h("span", { class: "muted" });
  private readonly status = h("p", { class: "status-line" });
  private readonly uploadChip = h("p", { class: "upload-chip" });
  private readonly summary: HTMLElement;
  private readonly summaryRows = h("dl", { class: "summary-grid" });
  private readonly details = h("details", { class: "card details" });
  private readonly detailsBody = h("div", { class: "details-body" });

  constructor() {
    this.banner.append(this.bannerText, this.retry);
    this.retry.addEventListener("click", () => void this.run(restartAgent));
    this.button.addEventListener("click", () => this.onButton());

    const chart = svg("svg", {
      class: "spark",
      viewBox: `0 0 ${CHART_W} ${CHART_H}`,
      preserveAspectRatio: "none",
      role: "img",
      "aria-label": "Total power over the last 5 minutes",
    });
    chart.append(this.chartArea, this.chartLine);
    this.chartCard = h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("span", {}, `Power, last ${WINDOW_SECS / 60} min`),
        this.chartMax,
      ),
      chart,
    );

    const openFolder = h("button", { class: "link", type: "button" }, "Open data folder");
    openFolder.addEventListener("click", () => void this.run(openDataFolder));
    this.summary = h(
      "section",
      { class: "card summary" },
      h("div", { class: "card-head" }, h("span", {}, "Last session"), openFolder),
      this.summaryRows,
    );

    this.details.append(h("summary", {}, "How is this measured?"), this.detailsBody);

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
        " Uploading to ",
        h("b", {}, target),
      );
      this.uploadChip.title = "Your measurements are sent to the CodeCarbon dashboard.";
    }
    show(this.uploadChip, target !== null);
  }

  update(state: TrackerState): void {
    this.state = state;
    const { phase } = state;
    const measuring = phase === "running" || phase === "stopping";

    // Error banner + loading hint.
    show(this.banner, state.error !== null);
    setText(this.bannerText, state.error ?? "");
    show(this.retry, phase === "agent_error");
    show(this.loading, state.agent_ready === null && phase !== "agent_error");

    // Start/Stop button.
    this.button.replaceChildren();
    this.button.classList.toggle("btn-stop", phase === "running");
    switch (phase) {
      case "starting":
        this.button.append(spinner(), " Starting…");
        break;
      case "stopping":
        this.button.append(spinner(), " Stopping…");
        break;
      case "running":
        this.button.append("Stop measuring");
        break;
      default:
        this.button.append("Start measuring");
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
      setText(this.power.sub, summary ? `average · ${breakdown}` : breakdown);
    } else {
      setText(this.power.sub, "");
    }
    setText(this.co2.value, emissions !== undefined ? fmt.co2(emissions) : "–");
    setText(this.energy.value, energy !== undefined ? fmt.energy(energy) : "–");
    setText(this.co2.sub, rate ? `${fmt.co2(rate * 3600)} per hour` : "");
    const ci =
      summary?.carbon_intensity_g_per_kwh ??
      m?.carbon_intensity_g_per_kwh ??
      state.location?.carbon_intensity_g_per_kwh ??
      null;
    setText(this.energy.sub, ci != null ? `grid: ${fmt.intensity(ci)}` : "");
    for (const t of [this.power, this.co2, this.energy, this.duration]) {
      t.root.classList.toggle("tile-live", measuring);
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
    setText(this.chartMax, geometry.line ? `max ${fmt.power(geometry.max)}` : "");
    show(this.chartCard, state.history.length > 1);

    // Status line.
    const line = statusLine(state.hardware, state.location, m?.carbon_intensity_g_per_kwh ?? null);
    setText(this.status, line);
    show(this.status, line !== "");

    // Last session summary.
    show(this.summary, summary !== null);
    if (summary) {
      const rows: [string, string][] = [
        ["Duration", fmt.duration(summary.duration_s)],
        ["Energy", fmt.energy(summary.energy_kwh.total)],
        ["CO₂e", fmt.co2(summary.emissions_kg)],
        ["Average power", fmt.power(summary.avg_power_w.total)],
      ];
      if (summary.carbon_intensity_g_per_kwh != null) {
        rows.push(["Grid intensity", fmt.intensity(summary.carbon_intensity_g_per_kwh)]);
      }
      this.summaryRows.replaceChildren(
        ...rows.flatMap(([k, v]) => [h("dt", {}, k), h("dd", {}, v)]),
      );
    }

    this.renderDetails(state);
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
    setText(this.duration.sub, util != null ? `CPU load ${util.toFixed(0)} %` : "");
  }

  private renderDetails(state: TrackerState): void {
    const hw = state.hardware;
    const items: HTMLElement[] = [];
    if (hw) {
      const cpu = [hw.cpu_model, hw.cpu_count ? `${hw.cpu_count} threads` : null]
        .filter(Boolean)
        .join(", ");
      items.push(h("li", {}, h("b", {}, "CPU: "), `${cpu || "unknown model"} — ${cpuMethod(hw)}`));
      if (hw.cpu_tdp_is_generic) {
        items.push(
          h("li", {}, "Your CPU model is not in CodeCarbon's database: a generic TDP is used."),
        );
      }
      if (hw.gpu_count && hw.gpu_count > 0) {
        items.push(
          h("li", {}, h("b", {}, "GPU: "), `${hw.gpu_model ?? "unknown"} — ${hw.gpu_method ?? "?"}`),
        );
      }
      if (hw.ram_total_gb != null) {
        items.push(
          h(
            "li",
            {},
            h("b", {}, "RAM: "),
            `${hw.ram_total_gb.toFixed(1)} GB — ${hw.ram_method ?? "estimate"}`,
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
          h("b", {}, "Electricity: "),
          `${where || "unknown location"}${ci != null ? ` — ${fmt.intensity(ci)}` : ""}`,
        ),
      );
    }
    const notes = state.notes.map((n) => h("li", { class: "note" }, n));
    const intro = h(
      "p",
      {},
      "CodeCarbon estimates the power of your CPU, GPU and RAM, multiplies the energy by the " +
        "carbon intensity of your country's electricity grid, and reports CO₂-equivalent emissions.",
    );
    const children: HTMLElement[] = [
      intro,
      items.length
        ? h("ul", {}, ...items)
        : h("p", { class: "muted" }, "Start a measurement to see your hardware."),
    ];
    if (notes.length) {
      children.push(h("p", { class: "muted" }, "Notes from CodeCarbon:"));
      children.push(h("ul", { class: "notes" }, ...notes));
    }
    this.detailsBody.replaceChildren(...children);
  }
}
