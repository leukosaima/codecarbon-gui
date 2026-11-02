/**
 * History: totals (today / this week / all time), a 30-day bar chart,
 * everyday equivalents and the list of past sessions, all read from
 * CodeCarbon's emissions.csv by `get_history` (history.rs).
 */
import {
  errorMessage,
  getHistory,
  openDataFolder,
  type DayTotal,
  type HistoryPayload,
  type HistorySession,
  type Settings,
  type Totals,
} from "../api";
import { barChart } from "../barchart";
import { h, setText, show, svg } from "../dom";
import { describe as describeEquivalent, ICONS } from "../equivalents";
import * as fmt from "../format";

/** Sessions shown at first, and added by every "Show more". */
const PAGE = 20;

const CHART_W = 300;
const BARS_H = 84;
const TICKS_H = 16;

export const EMPTY_TEXT = "Your sessions will appear here after you stop a measurement.";

/** A stat tile: CO₂e big, energy and time below. */
function statTile(label: string): { root: HTMLElement; update: (t: Totals) => void } {
  const value = h("div", { class: "tile-value" }, "–");
  const energy = h("div", { class: "tile-sub" });
  const time = h("div", { class: "tile-sub" });
  const root = h(
    "div",
    { class: "tile" },
    h("div", { class: "tile-label" }, label),
    value,
    energy,
    time,
  );
  return {
    root,
    update(t) {
      setText(value, fmt.co2(t.emissions_kg));
      setText(energy, fmt.energy(t.energy_kwh));
      const sessions = t.sessions === 1 ? "1 session" : `${t.sessions} sessions`;
      setText(time, t.sessions > 0 ? fmt.durationShort(t.duration_s) : "no session");
      root.title = `${label}: ${sessions}`;
    },
  };
}

function icon(key: string): SVGSVGElement {
  const el = svg("svg", { class: "eq-icon", viewBox: "0 0 20 20", "aria-hidden": "true" });
  el.append(svg("path", { d: ICONS[key] ?? "" }));
  return el;
}

export class HistoryView {
  readonly root: HTMLElement;

  private data: HistoryPayload | null = null;
  private shown = PAGE;
  /** Ignore the answer of a load that a newer one overtook. */
  private loadSeq = 0;

  private readonly error = h("div", { class: "banner", role: "alert" });
  private readonly csvOff = h(
    "p",
    { class: "hint small" },
    "Keeping a history is turned off in Settings: new sessions are not saved.",
  );
  private readonly empty = h("div", { class: "card empty-state" }, h("p", {}, EMPTY_TEXT));
  private readonly body: HTMLElement;
  private readonly today = statTile("Today");
  private readonly week = statTile("This week");
  private readonly allTime = statTile("All time");
  private readonly chart = svg("svg", {
    class: "bars",
    viewBox: `0 0 ${CHART_W} ${BARS_H + TICKS_H}`,
    role: "img",
    "aria-label": "CO₂e per day over the last 30 days",
  });
  private readonly chartMax = h("span", { class: "muted" });
  private readonly chartEmpty = h(
    "p",
    { class: "muted small chart-empty" },
    "No sessions in the last 30 days.",
  );
  private readonly equivalents = h("ul", { class: "equivalents" });
  private readonly sessions = h("ol", { class: "sessions" });
  private readonly sessionsCount = h("span", { class: "muted" });
  private readonly more = h("button", { class: "btn btn-small more", type: "button" }, "Show more");
  private readonly skipped = h("span", { class: "muted small" });

  constructor() {
    this.more.addEventListener("click", () => {
      this.shown += PAGE;
      this.renderSessions();
    });
    const openFolder = h("button", { class: "link", type: "button" }, "Open data folder");
    openFolder.addEventListener("click", () => {
      openDataFolder().catch((e: unknown) => this.showError(errorMessage(e)));
    });

    this.body = h(
      "div",
      { class: "view" },
      h("div", { class: "tiles tiles-3" }, this.today.root, this.week.root, this.allTime.root),
      h(
        "section",
        { class: "card" },
        h("div", { class: "card-head" }, h("span", {}, "Last 30 days"), this.chartMax),
        this.chart,
        this.chartEmpty,
      ),
      h(
        "section",
        { class: "card" },
        h(
          "div",
          { class: "card-head" },
          h("span", {}, "In everyday terms"),
          h("span", { class: "muted" }, "all time"),
        ),
        this.equivalents,
        h("p", { class: "muted small eq-note" }, "Orders of magnitude, from average factors."),
      ),
      h(
        "section",
        { class: "card" },
        h("div", { class: "card-head" }, h("span", {}, "Sessions"), this.sessionsCount),
        this.sessions,
        this.more,
      ),
    );

    this.root = h(
      "section",
      { class: "view history" },
      this.error,
      this.csvOff,
      this.empty,
      this.body,
      h("div", { class: "history-footer" }, openFolder, this.skipped),
    );
    show(this.error, false);
    show(this.csvOff, false);
    show(this.empty, false);
    show(this.body, false);
  }

  /** Saved settings: warn when the CSV history is turned off. */
  setSettings(settings: Settings): void {
    show(this.csvOff, !settings.save_csv);
  }

  /** Re-read emissions.csv (tab opened, or a session just stopped). */
  async load(): Promise<void> {
    const seq = ++this.loadSeq;
    try {
      const data = await getHistory();
      if (seq !== this.loadSeq) return;
      show(this.error, false);
      this.render(data);
    } catch (e) {
      if (seq === this.loadSeq) this.showError(errorMessage(e));
    }
  }

  private showError(message: string): void {
    setText(this.error, message);
    show(this.error, true);
  }

  render(data: HistoryPayload): void {
    this.data = data;
    const { summary } = data;
    const hasData = summary.all_time.sessions > 0;
    show(this.empty, !hasData);
    show(this.body, hasData);
    const skipped = data.skipped_rows;
    setText(
      this.skipped,
      skipped > 0 ? `${skipped} ${skipped === 1 ? "row" : "rows"} could not be read` : "",
    );
    this.skipped.title = skipped > 0 ? data.path : "";
    if (!hasData) return;

    this.today.update(summary.today);
    this.week.update(summary.this_week);
    this.allTime.update(summary.all_time);
    this.renderChart(summary.daily);

    this.equivalents.replaceChildren(
      ...data.equivalents_all_time.flatMap((eq) => {
        const text = describeEquivalent(eq);
        return text ? [h("li", {}, icon(eq.key), h("span", {}, text))] : [];
      }),
    );
    this.renderSessions();
  }

  private renderChart(daily: DayTotal[]): void {
    const { bars, max } = barChart(
      daily.map((d) => d.emissions_kg),
      CHART_W,
      BARS_H,
    );
    setText(this.chartMax, max > 0 ? `max ${fmt.co2(max)} / day` : "");
    show(this.chartEmpty, max === 0);
    const last = daily.length - 1;
    const nodes: SVGElement[] = [
      svg("line", { class: "bars-axis", x1: 0, x2: CHART_W, y1: BARS_H + 0.5, y2: BARS_H + 0.5 }),
    ];
    daily.forEach((day, i) => {
      const bar = bars[i];
      if (!bar) return;
      const g = svg("g", { class: i === last ? "bar today" : "bar" });
      const title = svg("title");
      const when = `${fmt.formatDay(day.date)}${i === last ? " (today)" : ""}`;
      title.textContent =
        `${when}: ${fmt.co2(day.emissions_kg)} CO₂e` +
        (day.duration_s > 0
          ? ` · ${fmt.energy(day.energy_kwh)} · ${fmt.durationShort(day.duration_s)}`
          : "");
      // A full-height transparent hit area, so empty days have a tooltip too.
      g.append(
        title,
        svg("rect", { class: "bar-hit", x: bar.x - 1, y: 0, width: bar.width + 2, height: BARS_H }),
        svg("rect", {
          class: "bar-fill",
          x: bar.x,
          y: bar.y,
          width: bar.width,
          height: bar.height,
          rx: 1,
        }),
      );
      nodes.push(g);
      // Weekday ticks: every Monday.
      if (fmt.parseLocal(day.date)?.getDay() === 1) {
        const cx = bar.x + bar.width / 2;
        const anchor = cx < 20 ? "start" : cx > CHART_W - 20 ? "end" : "middle";
        nodes.push(svg("line", { class: "bars-tick", x1: cx, x2: cx, y1: BARS_H, y2: BARS_H + 3 }));
        const label = svg("text", {
          class: "bars-label",
          x: anchor === "start" ? bar.x : anchor === "end" ? bar.x + bar.width : cx,
          y: BARS_H + TICKS_H - 3,
          "text-anchor": anchor,
        });
        label.textContent = fmt.formatTick(day.date);
        nodes.push(label);
      }
    });
    this.chart.replaceChildren(...nodes);
  }

  private renderSessions(): void {
    const all = this.data?.sessions ?? [];
    const visible = all.slice(0, this.shown);
    this.sessions.replaceChildren(...visible.map(sessionRow));
    const total = this.data?.summary.all_time.sessions ?? all.length;
    setText(this.sessionsCount, total === 1 ? "1 session" : `${total} sessions`);
    show(this.more, all.length > visible.length);
  }
}

function sessionRow(s: HistorySession): HTMLElement {
  const details = [fmt.durationShort(s.duration_s), fmt.energy(s.energy_kwh), s.country_name]
    .filter(Boolean)
    .join(" · ");
  return h(
    "li",
    { title: `${s.project_name || "session"} · average ${fmt.power(s.avg_power_w)}` },
    h(
      "div",
      { class: "session-main" },
      h("div", { class: "session-date" }, fmt.formatDate(s.started_at)),
      h("div", { class: "muted small" }, details),
    ),
    h("div", { class: "session-co2" }, fmt.co2(s.emissions_kg)),
  );
}
