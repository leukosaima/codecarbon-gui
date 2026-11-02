import { describe, expect, it } from "vitest";
import type { ApiSettings, Catalog } from "./api";
import {
  CREATE_NEW,
  emptyChoice,
  locateExperiment,
  resolveChoice,
  selectOptions,
  uploadChip,
  uploadTarget,
  userLabel,
} from "./account";

const catalog: Catalog = {
  organizations: [
    {
      id: "o1",
      name: "Org",
      projects: [
        {
          id: "p1",
          name: "Proj",
          experiments: [
            { id: "e1", name: "Laptop" },
            { id: "e2", name: "" },
          ],
        },
        { id: "p2", name: "Empty", experiments: [] },
      ],
    },
    { id: "o2", name: "Second", projects: [] },
  ],
};

const api = (over: Partial<ApiSettings> = {}): ApiSettings => ({
  enabled: true,
  endpoint: "https://api.codecarbon.io",
  api_key: "k",
  experiment_id: "e1",
  organization_name: "Org",
  project_name: "Proj",
  experiment_name: "Laptop",
  ...over,
});

describe("catalog → selects", () => {
  it("locates an experiment", () => {
    expect(locateExperiment(catalog, "e2")).toEqual({
      organizationId: "o1",
      projectId: "p1",
      experimentId: "e2",
    });
    expect(locateExperiment(catalog, "nope")).toBeNull();
    expect(locateExperiment(null, "e1")).toBeNull();
    expect(locateExperiment(catalog, "")).toBeNull();
  });

  it("defaults to the first organisation and project, never to an experiment", () => {
    expect(resolveChoice(catalog, emptyChoice())).toEqual({
      organizationId: "o1",
      projectId: "p1",
      experimentId: "",
    });
    expect(resolveChoice(catalog, { organizationId: "o2", projectId: "p1", experimentId: "e1" }))
      .toEqual({ organizationId: "o2", projectId: "", experimentId: "" });
    expect(resolveChoice(catalog, { organizationId: "gone", projectId: "", experimentId: "" }))
      .toEqual({ organizationId: "o1", projectId: "p1", experimentId: "" });
    expect(resolveChoice(null, { organizationId: "o1", projectId: "p1", experimentId: "e1" }))
      .toEqual(emptyChoice());
    expect(resolveChoice({ organizations: [] }, emptyChoice())).toEqual(emptyChoice());
  });

  it("builds the options, each select ending with Create new", () => {
    const o = selectOptions(catalog, { organizationId: "o1", projectId: "p1", experimentId: "" });
    expect(o.organizations.map((x) => x.value)).toEqual(["o1", "o2", CREATE_NEW]);
    expect(o.projects.map((x) => x.label)).toEqual(["Proj", "Empty", "＋ Create new…"]);
    expect(o.experiments.map((x) => x.value)).toEqual(["", "e1", "e2", CREATE_NEW]);
    expect(o.experiments[0]?.disabled).toBe(true);
    expect(o.experiments[2]?.label).toMatch(/^\(unnamed e2/);
  });

  it("explains empty levels", () => {
    const none = selectOptions({ organizations: [] }, emptyChoice());
    expect(none.organizations).toEqual([
      { value: "", label: "No organisation yet", disabled: true },
      { value: CREATE_NEW, label: "＋ Create new…" },
    ]);
    expect(none.projects.map((x) => x.value)).toEqual([""]); // nothing to create into
    expect(none.experiments.map((x) => x.value)).toEqual([""]);
    const empty = selectOptions(catalog, { organizationId: "o1", projectId: "p2", experimentId: "" });
    expect(empty.experiments.map((x) => x.label)).toEqual(["No experiment yet", "＋ Create new…"]);
  });
});

describe("labels", () => {
  it("user", () => {
    expect(userLabel({ name: "Ada", email: "ada@example.org" })).toBe("Ada (ada@example.org)");
    expect(userLabel({ name: null, email: "ada@example.org" })).toBe("ada@example.org");
    expect(userLabel({ name: " ", email: null })).toBe("your CodeCarbon account");
    expect(userLabel(null)).toBe("your CodeCarbon account");
  });

  it("upload target and dashboard chip", () => {
    expect(uploadTarget(api())).toBe("Org / Proj / Laptop");
    expect(uploadTarget(api({ organization_name: "", project_name: "", experiment_name: "" })))
      .toBe("experiment e1");
    expect(uploadChip(api())).toBe("Proj / Laptop");
    expect(uploadChip(api({ project_name: "", experiment_name: "" }))).toBe(
      "the CodeCarbon dashboard",
    );
    expect(uploadChip(api({ enabled: false }))).toBeNull();
    expect(uploadChip(api({ api_key: "" }))).toBeNull();
    expect(uploadChip(api({ experiment_id: " " }))).toBeNull();
    expect(uploadChip(null)).toBeNull();
  });
});
