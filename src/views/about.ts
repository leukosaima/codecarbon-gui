/** About: versions, updates, links, licence. */
import { getVersion } from "@tauri-apps/api/app";
import { checkForUpdates, errorMessage, openUrl, quitApp, type TrackerState } from "../api";
import { h, setText, show } from "../dom";
import { isAppError, live, type MessageKey } from "../i18n/i18n";
import { updateStatusLine } from "../update";
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
  private readonly checkButton = h(
    "button",
    { class: "btn btn-small", type: "button" },
    live("about.check_updates"),
  );
  /** "You are up to date.", "Version 0.2.0 available.", "Update check failed: …" */
  private readonly updateLine = h("p", { class: "muted small update-line", role: "status" });

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
    this.checkButton.addEventListener("click", () => {
      this.checkButton.disabled = true;
      setText(this.message, "");
      // The result is rendered from the state (`update`); a failed check
      // is in `updater.error`, anything else is shown here.
      checkForUpdates().catch((e: unknown) => {
        if (!(isAppError(e) && e.code === "update_check_failed")) {
          setText(this.message, errorMessage(e));
        }
      });
    });

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
      h(
        "section",
        { class: "card about-updates" },
        h(
          "div",
          { class: "card-head" },
          h("span", {}, live("about.updates")),
          this.checkButton,
        ),
        this.updateLine,
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
    setText(this.updateLine, updateStatusLine(state));
    show(this.updateLine, this.updateLine.textContent !== "");
    show(this.checkButton, state.updater.configured);
    this.checkButton.disabled = state.updater.phase !== "idle";
  }
}
