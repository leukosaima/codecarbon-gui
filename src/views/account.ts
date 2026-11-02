/**
 * "CodeCarbon dashboard" card of the Settings view: sign in, pick (or
 * create) an organisation / project / experiment, and turn uploads on.
 *
 * Rendering comes from `TrackerState.account` (pushed by Rust) and the saved
 * `Settings`; the only local state is what is chosen in the selects and
 * which "Create new…" row is open.
 */
import {
  accountCancelSignIn,
  accountCreate,
  accountRefreshCatalog,
  accountSelectExperiment,
  accountSignIn,
  accountSignOut,
  errorMessage,
  openUrl,
  type ApiKind,
  type Settings,
  type TrackerState,
} from "../api";
import {
  CREATE_NEW,
  emptyChoice,
  hasUploadTarget,
  locateExperiment,
  resolveChoice,
  selectOptions,
  uploadTarget,
  userLabel,
  type Choice,
  type SelectOption,
} from "../account";
import { h, setText, show, spinner } from "../dom";
import { live, liveAttr, t, tNodes, type MessageKey } from "../i18n/i18n";

const DASHBOARD_URL = "https://dashboard.codecarbon.io";

/** Separate sentences per kind: French nouns have a gender. */
const NEW_NAME: Record<ApiKind, MessageKey> = {
  organization: "account.new_name.organization",
  project: "account.new_name.project",
  experiment: "account.new_name.experiment",
};
const NAME_REQUIRED: Record<ApiKind, MessageKey> = {
  organization: "account.name_required.organization",
  project: "account.name_required.project",
  experiment: "account.name_required.experiment",
};

interface Level {
  kind: ApiKind;
  select: HTMLSelectElement;
  row: HTMLElement;
  createRow: HTMLElement;
  createInput: HTMLInputElement;
  createButton: HTMLButtonElement;
}

/** A link-styled button; `label` is a message key or a literal name. */
function link(label: MessageKey | { name: string }, onClick: () => void): HTMLButtonElement {
  const text = typeof label === "string" ? live(label) : label.name;
  const button = h("button", { class: "link", type: "button" }, text);
  button.addEventListener("click", onClick);
  return button;
}

export class AccountCard {
  readonly root: HTMLElement;
  /** Where the Settings view puts the manual endpoint / key / experiment fields. */
  readonly advanced: HTMLElement;

  private state: TrackerState | null = null;
  private settings: Settings | null = null;
  private choice: Choice = emptyChoice();
  /** experiment id + catalog presence the choice was last synced from. */
  private syncedFrom = "";
  private renderedSelects = "";
  /** The open "Create new…" row. */
  private createKind: ApiKind | null = null;
  /** `api_create` sent for this kind; the created item gets selected. */
  private pendingCreate: ApiKind | null = null;
  private seenCreated: string | null = null;
  /** An error of the last action (re-rendered in the current language). */
  private localError: (() => string) | null = null;
  private toggling = false;

  private readonly checking = h("p", { class: "hint account-hint" });
  private readonly signedOut: HTMLElement;
  private readonly signingIn: HTMLElement;
  private readonly signedIn: HTMLElement;
  private readonly reopen: HTMLButtonElement;
  private readonly userName = h("b");
  private readonly signedInAs = h("span");
  private readonly levels: Record<ApiKind, Level>;
  private readonly refresh: HTMLButtonElement;
  private readonly catalogStatus = h("span", { class: "muted small account-status" });
  private readonly upload: HTMLElement;
  private readonly uploadToggle = h("input", {
    id: "f-api-enabled",
    type: "checkbox",
    role: "switch",
  });
  private readonly target = h("p", { class: "muted small account-target" });
  private readonly busyHint = h("p", { class: "muted small" }, live("account.busy_hint"));
  private readonly error = h("p", { class: "field-error account-error", role: "alert" });

  constructor(private readonly saveUploadEnabled: (enabled: boolean) => Promise<void>) {
    const signIn = h(
      "button",
      { class: "btn btn-primary account-cta", type: "button" },
      live("account.sign_in"),
    );
    signIn.addEventListener("click", () => void this.run(accountSignIn));
    this.signedOut = h(
      "div",
      { class: "account-block" },
      h("p", {}, live("account.pitch")),
      signIn,
      h(
        "p",
        { class: "small account-site" },
        link({ name: "dashboard.codecarbon.io" }, () => void this.run(() => openUrl(DASHBOARD_URL))),
      ),
    );

    this.reopen = link("account.reopen", () => {
      const url = this.state?.account.auth_url;
      if (url) void this.run(() => openUrl(url));
    });
    const cancel = h("button", { class: "btn btn-small", type: "button" }, live("common.cancel"));
    cancel.addEventListener("click", () => void this.run(accountCancelSignIn));
    this.signingIn = h(
      "div",
      { class: "account-block" },
      h("p", { class: "hint account-hint" }, spinner(), live("account.finish_in_browser")),
      h("div", { class: "account-actions" }, this.reopen, cancel),
    );

    this.levels = {
      organization: this.level("organization", "account.organization"),
      project: this.level("project", "account.project"),
      experiment: this.level("experiment", "account.experiment"),
    };
    this.refresh = link("account.refresh", () => void this.run(accountRefreshCatalog));
    this.signedIn = h(
      "div",
      { class: "account-block" },
      h(
        "p",
        { class: "account-user" },
        this.signedInAs,
        link("account.sign_out", () => void this.run(accountSignOut)),
      ),
      ...Object.values(this.levels).flatMap((l) => [l.row, l.createRow]),
      h("div", { class: "account-actions" }, this.catalogStatus, this.refresh),
    );

    this.uploadToggle.addEventListener("change", () => void this.toggleUpload());
    this.upload = h(
      "div",
      { class: "account-block" },
      h(
        "div",
        { class: "field field-toggle" },
        h("label", { for: "f-api-enabled" }, h("span", {}, live("account.upload"))),
        this.uploadToggle,
      ),
      this.target,
      this.busyHint,
    );

    this.advanced = h("div", { class: "details-body" });
    this.root = h(
      "section",
      { class: "card account", "aria-labelledby": "account-title" },
      h("h2", { class: "card-title", id: "account-title" }, live("account.title")),
      this.checking,
      this.signedOut,
      this.signingIn,
      this.signedIn,
      this.upload,
      this.error,
      h(
        "details",
        { class: "details account-advanced" },
        h("summary", {}, live("common.advanced")),
        this.advanced,
      ),
    );
    this.render();
  }

  /** A labelled select + its hidden "Create new…" row. */
  private level(kind: ApiKind, label: MessageKey): Level {
    const id = `f-account-${kind}`;
    const select = h("select", { id });
    const row = h("div", { class: "field" }, h("label", { for: id }, live(label)), select);
    const createInput = h("input", { type: "text", maxlength: 100, spellcheck: false });
    liveAttr(createInput, "placeholder", NEW_NAME[kind]);
    liveAttr(createInput, "aria-label", NEW_NAME[kind]);
    const createButton = h(
      "button",
      { class: "btn btn-small btn-primary", type: "button" },
      live("account.create"),
    );
    const createRow = h(
      "div",
      { class: "account-create" },
      createInput,
      createButton,
      link("common.cancel", () => this.closeCreate()),
    );
    createRow.hidden = true;
    select.addEventListener("change", () => this.onSelect(kind, select.value));
    createButton.addEventListener("click", () => void this.create(kind));
    createInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault(); // not the settings form's submit
        void this.create(kind);
      } else if (e.key === "Escape") {
        this.closeCreate();
      }
    });
    return { kind, select, row, createRow, createInput, createButton };
  }

  update(state: TrackerState): void {
    this.state = state;
    const created = state.account.last_created;
    if (created && created.id !== this.seenCreated) {
      this.seenCreated = created.id;
      if (this.pendingCreate === created.kind) this.selectCreated(created.kind as ApiKind, created.id);
    }
    if (!state.account.creating && this.pendingCreate && state.account.last_error) {
      this.pendingCreate = null; // creation failed: keep the row open, show the error
    }
    this.render();
  }

  setSettings(settings: Settings): void {
    this.settings = settings;
    this.render();
  }

  /** The language changed: rebuild the selects' options, render again. */
  relocalize(): void {
    this.renderedSelects = "";
    this.render();
  }

  private async run(action: () => Promise<void>): Promise<void> {
    this.localError = null;
    try {
      await action();
    } catch (e) {
      this.localError = () => errorMessage(e);
    }
    this.render();
  }

  private onSelect(kind: ApiKind, value: string): void {
    this.localError = null;
    if (value === CREATE_NEW) {
      this.createKind = kind;
      const level = this.levels[kind];
      level.createInput.value = "";
      this.render();
      level.createInput.focus();
      return;
    }
    this.createKind = null;
    if (kind === "organization") {
      this.choice = { organizationId: value, projectId: "", experimentId: "" };
    } else if (kind === "project") {
      this.choice = { ...this.choice, projectId: value, experimentId: "" };
    } else {
      this.choice = { ...this.choice, experimentId: value };
      this.selectExperiment();
    }
    this.render();
  }

  private selectExperiment(): void {
    const { organizationId, projectId, experimentId } = this.choice;
    if (!experimentId || experimentId === this.settings?.api.experiment_id) return;
    void this.run(() => accountSelectExperiment(organizationId, projectId, experimentId));
  }

  private closeCreate(): void {
    this.createKind = null;
    this.pendingCreate = null;
    this.renderedSelects = ""; // put the select back on the chosen value
    this.render();
  }

  private async create(kind: ApiKind): Promise<void> {
    const name = this.levels[kind].createInput.value.trim();
    if (!name) {
      this.localError = () => t(NAME_REQUIRED[kind]);
      this.render();
      return;
    }
    const parent =
      kind === "project"
        ? this.choice.organizationId
        : kind === "experiment"
          ? this.choice.projectId
          : null;
    this.pendingCreate = kind;
    await this.run(() => accountCreate(kind, name, parent));
    if (this.localError !== null) this.pendingCreate = null;
  }

  /** The item we asked for was created: choose it. */
  private selectCreated(kind: ApiKind, id: string): void {
    this.pendingCreate = null;
    this.createKind = null;
    if (kind === "organization") {
      this.choice = { organizationId: id, projectId: "", experimentId: "" };
    } else if (kind === "project") {
      this.choice = { ...this.choice, projectId: id, experimentId: "" };
    } else {
      this.choice = { ...this.choice, experimentId: id };
      this.selectExperiment();
    }
  }

  private async toggleUpload(): Promise<void> {
    const enabled = this.uploadToggle.checked;
    this.toggling = true;
    this.render();
    await this.run(() => this.saveUploadEnabled(enabled));
    this.toggling = false;
    this.render();
  }

  private render(): void {
    const account = this.state?.account;
    const phase = account?.phase ?? "unknown";
    const ready = this.state?.agent_ready ?? null;

    show(this.checking, phase === "unknown");
    if (phase === "unknown") {
      this.checking.replaceChildren();
      if (ready && ready.protocol_version < 2) {
        this.checking.append(t("account.engine_too_old"));
      } else {
        this.checking.append(
          spinner(),
          t(ready ? "account.checking" : "common.starting_engine"),
        );
      }
    }
    show(this.signedOut, phase === "signed_out");
    show(this.signingIn, phase === "signing_in");
    show(this.reopen, Boolean(account?.auth_url));
    show(this.signedIn, phase === "signed_in");
    if (account && phase === "signed_in") {
      setText(this.userName, userLabel(account.user));
      this.signedInAs.replaceChildren(
        ...tNodes("account.signed_in_as", { user: this.userName }),
      );
      this.renderSelects();
    }

    const api = this.settings?.api ?? null;
    const target = api !== null && hasUploadTarget(api);
    show(this.upload, phase === "signed_in" || target);
    if (!this.toggling) this.uploadToggle.checked = api?.enabled ?? false;
    this.uploadToggle.disabled = !target || this.toggling;
    this.target.replaceChildren();
    if (api && target) {
      this.target.append(
        ...tNodes(api.enabled ? "account.uploads_to" : "account.uploads_off", {
          target: h("b", {}, uploadTarget(api)),
        }),
      );
    } else {
      this.target.append(t("account.choose_target"));
    }
    const phaseNow = this.state?.phase;
    show(
      this.busyHint,
      phaseNow === "running" || phaseNow === "starting" || phaseNow === "stopping",
    );

    const error =
      this.localError?.() ?? (account?.last_error ? errorMessage(account.last_error) : "");
    setText(this.error, error);
    show(this.error, error !== "");
  }

  private renderSelects(): void {
    const account = this.state?.account;
    if (!account) return;
    const catalog = account.catalog;
    const experimentId = this.settings?.api.experiment_id ?? "";
    // Follow the saved experiment when it (or the catalog) changes.
    const syncKey = `${experimentId}|${catalog ? "catalog" : ""}`;
    if (syncKey !== this.syncedFrom) {
      this.syncedFrom = syncKey;
      const located = locateExperiment(catalog, experimentId);
      if (located) this.choice = located;
    }
    this.choice = resolveChoice(catalog, this.choice);

    const saving = account.saving !== null;
    const busy = saving || account.creating;
    const key = JSON.stringify([catalog, this.choice, this.createKind]);
    if (key !== this.renderedSelects) {
      // Only when something changed: rebuilding closes an open dropdown.
      this.renderedSelects = key;
      const options = selectOptions(catalog, this.choice);
      const fill = (level: Level, opts: SelectOption[], value: string): void => {
        level.select.replaceChildren(
          ...opts.map((o) => h("option", { value: o.value, disabled: o.disabled ?? false }, o.label)),
        );
        level.select.value = this.createKind === level.kind ? CREATE_NEW : value;
      };
      fill(this.levels.organization, options.organizations, this.choice.organizationId);
      fill(this.levels.project, options.projects, this.choice.projectId);
      fill(this.levels.experiment, options.experiments, this.choice.experimentId);
    }
    for (const level of Object.values(this.levels)) {
      const enabled = level.select.options.length > 1 || level.select.value === CREATE_NEW;
      level.select.disabled = busy || catalog === null || !enabled;
      level.createRow.hidden = this.createKind !== level.kind;
      level.createInput.disabled = account.creating;
      level.createButton.disabled = account.creating;
    }

    this.catalogStatus.replaceChildren();
    if (saving) this.catalogStatus.append(spinner(), ` ${t("account.saving")}`);
    else if (account.creating) this.catalogStatus.append(spinner(), ` ${t("account.creating")}`);
    else if (account.catalog_loading) {
      this.catalogStatus.append(spinner(), ` ${t("account.loading")}`);
    }
    this.refresh.disabled = account.catalog_loading || busy;
  }
}
