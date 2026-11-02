/**
 * Pure helpers of the "CodeCarbon dashboard" card: catalog → select
 * options, and the labels shown in the Settings view and on the dashboard.
 */
import type { ApiSettings, Catalog, Organization, Project, User } from "./api";
import { t } from "./i18n/i18n";

/** Value of the last option of each select ("＋ Create new…"). */
export const CREATE_NEW = "__create_new__";

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

/** What is chosen in the three selects ("" = nothing). */
export interface Choice {
  organizationId: string;
  projectId: string;
  experimentId: string;
}

export const emptyChoice = (): Choice => ({ organizationId: "", projectId: "", experimentId: "" });

const label = (name: string, id: string): string =>
  name.trim() || t("account.unnamed", { id: id.slice(0, 8) });

/** Where an experiment lives in the catalog, or null. */
export function locateExperiment(catalog: Catalog | null, experimentId: string): Choice | null {
  if (!catalog || !experimentId) return null;
  for (const org of catalog.organizations) {
    for (const project of org.projects) {
      if (project.experiments.some((e) => e.id === experimentId)) {
        return { organizationId: org.id, projectId: project.id, experimentId };
      }
    }
  }
  return null;
}

function findOrg(catalog: Catalog, id: string): Organization | undefined {
  return catalog.organizations.find((o) => o.id === id);
}

function findProject(org: Organization | undefined, id: string): Project | undefined {
  return org?.projects.find((p) => p.id === id);
}

/**
 * Make a choice consistent with the catalog: ids that do not exist (any
 * more) are dropped, and a missing organisation / project defaults to the
 * first one. The experiment never defaults: choosing one saves it.
 */
export function resolveChoice(catalog: Catalog | null, choice: Choice): Choice {
  if (!catalog) return emptyChoice();
  const org = findOrg(catalog, choice.organizationId) ?? catalog.organizations[0];
  const project = findProject(org, choice.projectId) ?? org?.projects[0];
  const experiment = project?.experiments.find((e) => e.id === choice.experimentId);
  return {
    organizationId: org?.id ?? "",
    projectId: project?.id ?? "",
    experimentId: experiment?.id ?? "",
  };
}

/** Options of the Organisation, Project and Experiment selects. */
export function selectOptions(
  catalog: Catalog | null,
  choice: Choice,
): { organizations: SelectOption[]; projects: SelectOption[]; experiments: SelectOption[] } {
  const create: SelectOption = { value: CREATE_NEW, label: t("account.create_new") };
  const orgs = catalog?.organizations ?? [];
  const org = catalog ? findOrg(catalog, choice.organizationId) : undefined;
  const project = findProject(org, choice.projectId);

  const organizations: SelectOption[] = orgs.length
    ? orgs.map((o) => ({ value: o.id, label: label(o.name, o.id) }))
    : [{ value: "", label: t("account.no_organization"), disabled: true }];
  organizations.push(create);

  const projects: SelectOption[] = org?.projects.length
    ? org.projects.map((p) => ({ value: p.id, label: label(p.name, p.id) }))
    : [
        {
          value: "",
          label: t(org ? "account.no_project" : "account.choose_organization_first"),
          disabled: true,
        },
      ];
  if (org) projects.push(create);

  const experiments: SelectOption[] = [];
  if (project?.experiments.length) {
    experiments.push({ value: "", label: t("account.choose_experiment"), disabled: true });
    experiments.push(...project.experiments.map((e) => ({ value: e.id, label: label(e.name, e.id) })));
  } else {
    const text = t(project ? "account.no_experiment" : "account.choose_project_first");
    experiments.push({ value: "", label: text, disabled: true });
  }
  if (project) experiments.push(create);

  return { organizations, projects, experiments };
}

/** "Ada Lovelace (ada@example.org)", or whichever part is known. */
export function userLabel(user: User | null): string {
  const name = user?.name?.trim() ?? "";
  const email = user?.email?.trim() ?? "";
  if (name && email) return `${name} (${email})`;
  return name || email || t("account.your_account");
}

/** True when the settings hold everything an upload needs. */
export function hasUploadTarget(api: ApiSettings): boolean {
  return api.api_key.trim() !== "" && api.experiment_id.trim() !== "";
}

/** "Org / Project / Experiment" (names saved with the selection). */
export function uploadTarget(api: ApiSettings): string {
  const names = [api.organization_name, api.project_name, api.experiment_name]
    .map((n) => n.trim())
    .filter(Boolean);
  return names.length
    ? names.join(" / ")
    : t("account.experiment_id", { id: api.experiment_id.trim() });
}

/** Text of the dashboard chip ("project / experiment"), or null when not uploading. */
export function uploadChip(api: ApiSettings | null): string | null {
  if (!api || !api.enabled || !hasUploadTarget(api)) return null;
  const names = [api.project_name, api.experiment_name].map((n) => n.trim()).filter(Boolean);
  return names.length ? names.join(" / ") : t("account.the_dashboard");
}
