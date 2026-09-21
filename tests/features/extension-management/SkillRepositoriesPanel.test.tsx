import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { skillRepositoryKeys } from "@/entities/skill-repository";
import { SkillRepositoriesPanel } from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const REPOSITORY = {
  id: "a".repeat(64),
  owner: "anthropics",
  repository: "skills",
  branch: "main",
  enabled: true,
};

function mount() {
  const client = createTestQueryClient();
  return {
    client,
    ...render(<SkillRepositoriesPanel />, {
      wrapper: withQueryClient(client),
    }),
  };
}

describe("SkillRepositoriesPanel", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, extensions: en.extensions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_repositories_list`, () =>
        HttpResponse.json([REPOSITORY]),
      ),
    );
  });

  it("focuses the guided GitHub field and renders configured sources", async () => {
    mount();
    const location = screen.getByLabelText(
      en.extensions.skill.repositories.location,
    );
    expect(location).toHaveFocus();
    expect(await screen.findByText("anthropics/skills")).toBeInTheDocument();
    expect(screen.getByText("Branch: main")).toBeInTheDocument();
  });

  it("normalizes a new source, updates the session cache, and refreshes the catalog", async () => {
    let body: unknown;
    const added = {
      id: "b".repeat(64),
      owner: "example-org",
      repository: "example-skills",
      branch: "HEAD",
      enabled: true,
    };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_repository_save`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json([REPOSITORY, added]);
        },
      ),
    );
    const { client } = mount();
    await screen.findByText("anthropics/skills");

    await userEvent.type(
      screen.getByLabelText(en.extensions.skill.repositories.location),
      "https://github.com/example-org/example-skills.git",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.repositories.add,
      }),
    );

    await waitFor(() =>
      expect(body).toEqual({
        repository: {
          owner: "example-org",
          repository: "example-skills",
          branch: "HEAD",
          enabled: true,
        },
      }),
    );
    expect(
      await screen.findByText("example-org/example-skills"),
    ).toBeInTheDocument();
    expect(client.getQueryData(skillRepositoryKeys.list())).toEqual([
      REPOSITORY,
      added,
    ]);
  });

  it("toggles a source with the complete validated draft", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_repository_save`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json([{ ...REPOSITORY, enabled: false }]);
        },
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("switch", {
        name: "Disable Skill source anthropics/skills",
      }),
    );
    await waitFor(() =>
      expect(body).toEqual({
        repository: {
          owner: "anthropics",
          repository: "skills",
          branch: "main",
          enabled: false,
        },
      }),
    );
    expect(
      await screen.findByRole("switch", {
        name: "Enable Skill source anthropics/skills",
      }),
    ).not.toBeChecked();
  });

  it("confirms removal inline and sends only the opaque source id", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_repository_remove`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json([]);
        },
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Remove Skill source anthropics/skills",
      }),
    );
    const confirmation = screen.getByRole("alertdialog", {
      name: "Remove anthropics/skills from Skill sources?",
    });
    expect(confirmation).toHaveTextContent(
      "Skills already installed from it stay installed.",
    );
    await userEvent.click(
      within(confirmation).getByRole("button", { name: "Remove source" }),
    );
    await waitFor(() => expect(body).toEqual({ repository: REPOSITORY.id }));
    expect(
      await screen.findByText(en.extensions.skill.repositories.empty),
    ).toBeInTheDocument();
  });

  it("shows stable failure copy without exposing native paths or credentials", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_repository_save`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_PARSE_FAILED",
            messageKey: "error.skill.repositoryInvalid",
            technicalMessage: "/private/repo token=do-not-show",
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await screen.findByText("anthropics/skills");
    await userEvent.type(
      screen.getByLabelText(en.extensions.skill.repositories.location),
      "example-org/example-skills",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.repositories.add,
      }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.error.skill.repositoryInvalid);
    expect(alert).toHaveTextContent(
      en.extensions.skill.repositories.mutationErrorHint,
    );
    expect(alert).not.toHaveTextContent("/private/repo");
    expect(alert).not.toHaveTextContent("do-not-show");
  });
});
