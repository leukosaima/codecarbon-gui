/** About: versions, links, licence. */
import { getVersion } from "@tauri-apps/api/app";
import { errorMessage, openUrl, quitApp, type TrackerState } from "../api";
import { h, setText } from "../dom";
import logoUrl from "../../src-tauri/icons/128x128@2x.png";

const LINKS: [string, string][] = [
  ["codecarbon.io", "https://codecarbon.io"],
  ["CodeCarbon Desktop on GitHub", "https://github.com/mlco2/codecarbon-gui"],
  ["CodeCarbon library on GitHub", "https://github.com/mlco2/codecarbon"],
];

export class AboutView {
  readonly root: HTMLElement;
  private readonly appVersion = h("dd", {}, "…");
  private readonly agentVersion = h("dd", {}, "–");
  private readonly codecarbonVersion = h("dd", {}, "–");
  private readonly pythonVersion = h("dd", {}, "–");
  private readonly platform = h("dd", {}, "–");
  private readonly message = h("p", { class: "form-message error", role: "status" });

  constructor() {
    const links = LINKS.map(([label, url]) => {
      const a = h("a", { href: url, class: "link" }, label);
      a.addEventListener("click", (e) => {
        e.preventDefault();
        openUrl(url).catch((err: unknown) => setText(this.message, errorMessage(err)));
      });
      return h("li", {}, a);
    });
    const quit = h("button", { class: "btn", type: "button" }, "Quit CodeCarbon");
    quit.addEventListener("click", () => void quitApp());

    this.root = h(
      "section",
      { class: "view about" },
      h(
        "div",
        { class: "about-head" },
        h("img", { src: logoUrl, alt: "", width: 64, height: 64 }),
        h(
          "div",
          {},
          h("h1", {}, "CodeCarbon"),
          h("p", { class: "muted" }, "Measure the power and CO₂ emissions of your computer."),
        ),
      ),
      h(
        "dl",
        { class: "card summary-grid" },
        h("dt", {}, "App"),
        this.appVersion,
        h("dt", {}, "Agent"),
        this.agentVersion,
        h("dt", {}, "CodeCarbon"),
        this.codecarbonVersion,
        h("dt", {}, "Python"),
        this.pythonVersion,
        h("dt", {}, "Platform"),
        this.platform,
      ),
      h("ul", { class: "links" }, ...links),
      h(
        "p",
        { class: "muted small" },
        "Released under the MIT licence. Numbers are computed by the CodeCarbon library; " +
          "they are estimates, not lab measurements.",
      ),
      this.message,
      h("div", { class: "form-actions" }, quit),
    );

    getVersion()
      .then((v) => setText(this.appVersion, v))
      .catch(() => setText(this.appVersion, "–"));
  }

  update(state: TrackerState): void {
    const r = state.agent_ready;
    setText(this.agentVersion, r?.agent_version || "–");
    setText(this.codecarbonVersion, r?.codecarbon_version || "–");
    setText(this.pythonVersion, r?.python_version || "–");
    setText(this.platform, r?.platform || "–");
  }
}
