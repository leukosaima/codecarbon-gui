/** About: versions, links, licence. */
import { getVersion } from "@tauri-apps/api/app";
import { errorMessage, openUrl, quitApp, type TrackerState } from "../api";
import { h, setText } from "../dom";
import { live, type MessageKey } from "../i18n/i18n";
import logoUrl from "../../src-tauri/icons/128x128@2x.png";

/** Label (a message key, or a literal name), URL. */
const LINKS: [MessageKey | { name: string }, string][] = [
  [{ name: "codecarbon.io" }, "https://codecarbon.io"],
  ["about.link_desktop", "https://github.com/mlco2/codecarbon-gui"],
  ["about.link_library", "https://github.com/mlco2/codecarbon"],
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
      const text = typeof label === "string" ? live(label) : label.name;
      const a = h("a", { href: url, class: "link" }, text);
      a.addEventListener("click", (e) => {
        e.preventDefault();
        openUrl(url).catch((err: unknown) => setText(this.message, errorMessage(err)));
      });
      return h("li", {}, a);
    });
    const quit = h("button", { class: "btn", type: "button" }, live("about.quit"));
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
          h("p", { class: "muted" }, live("about.tagline")),
        ),
      ),
      h(
        "dl",
        { class: "card summary-grid" },
        h("dt", {}, live("about.app")),
        this.appVersion,
        h("dt", {}, live("about.agent")),
        this.agentVersion,
        h("dt", {}, "CodeCarbon"),
        this.codecarbonVersion,
        h("dt", {}, "Python"),
        this.pythonVersion,
        h("dt", {}, live("about.platform")),
        this.platform,
      ),
      h("ul", { class: "links" }, ...links),
      h("p", { class: "muted small" }, live("about.licence")),
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
